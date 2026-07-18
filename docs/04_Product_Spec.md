# Product Specification

---

## 0. How to Read This Document

This document defines observable behavior, exact copy, and acceptance criteria for each v1 screen shown in the redesign mockups (`docs/mockups/`). It is subordinate to [01_Vision.md](01_Vision.md) and [02_Architecture.md](02_Architecture.md). Structure is fixed by [Architecture §9](02_Architecture.md#9-widget-tree-source-of-truth); state wiring by [Architecture §10](02_Architecture.md#10-state--ui-mapping); look by [05_Visual_Design.md](05_Visual_Design.md).

`Must`/`must not` are requirements. `Should` is a strong default. `May` is optional. Every quoted string is the exact copy to ship. A state is complete only when its acceptance criteria pass **and** its Architecture constraints hold.

**Out of scope for v1 — present in mockups, deliberately not built** (Vision §5): star ratings; EXIF fields (Date taken, Camera, Lens, ISO, Focal length, Aperture, Exposure); GPS/Location; content hashes (MD5/SHA256); manual/editable tags ("+ Add tag", tag removal); collections ("Add to Collection"); Date/Type/Rating filter chips; list view; grouped sidebar sections; a Settings "Metadata" page. These must not appear in the shipped UI even though a mockup depicts them.

---

## 1. Global Shell

Every non-viewer screen is the same shell: a fixed **220px** sidebar (collapsible) plus the grid area (header + status stack + content stack). See Architecture §9.

**Sidebar (top → bottom):**

- Brand block: app icon, "Vesper", subtitle "Quiet nocturne media gallery", and a collapse control (`«`).
- Tag list: a **flat** list of folder-derived tags, each row showing the tag's display name and its online visible file count, sorted by count descending, then case-insensitive name, then path identity (Architecture §3). Rows with colliding display names disambiguate via breadcrumb secondary text or tooltip. There are no hand-authored sections or smart collections; names like "Favorites" or "RAW Imports" appear only if such folders exist on disk.
- Footer: primary "Add Source Root" button and a settings (gear) button.

**Header (grid area):** sidebar-restore toggle (`☰`, visible only when the sidebar is collapsed), search entry, "Sort" dropdown, thumbnail-size control, a selection toggle, primary menu, and window controls.

**Content stack** shows exactly one of: `empty`, `no-results`, `grid` (Architecture §9 `root_stack`). The `status_banner_stack` sits above the content and shows at most one banner by priority: (1) recoverable critical state, (2) offline roots, (3) scan/indexing active. Scan warnings remain reachable independently via the bottom-left scan-issue indicator even when a higher banner shows.

**Acceptance:**

- The sidebar width is exactly 220px expanded and is never user-resizable (no drag handle).
- The content stack shows exactly one page at a time; switching pages crossfades (Visual §6).
- Tag counts reflect only online, visible media; offline-root tags are excluded from counts (Architecture §1, §3).

---

## 2. Main Grid — Browsing

Reference: `01_main_gallery.png`.

**Behavior:**

- The grid header shows the current grouping title and count, e.g. "July 2023" (`title-2`) over "318 items" (`caption`, `muted`).
- Each cell shows a square 256px-sourced thumbnail. Video cells carry a duration badge ("0:14", "0:42", "0:08") bottom-left. A cell whose thumbnail failed shows a stable placeholder, never a broken image or an error dialog (Vision §2).
- **Search**: the header search entry has placeholder "Search Vesper" and a `/` shortcut hint. Search is case-insensitive and Unicode-normalized (Architecture §4) and clears on every launch (Architecture §8). Typing supersedes older queries; only the newest result is applied.
- **Sort**: a "Sort: Date ▾" dropdown. Sort order persists across launches (Architecture §8).
- **Thumbnail size**: a control opening a "Thumbnail size" popover with exactly five options and shortcuts — "Extra Small" `Ctrl+1`, "Small" `Ctrl+2`, "Medium" `Ctrl+3`, "Large" `Ctrl+4`, "Extra Large" `Ctrl+5`. The active size is highlighted. Zoom level persists across launches. These are the only five sizes (Vision §6).
- **Scroll**: position is restored on launch via a stable anchor after window size, zoom, sort, and filters are restored (Architecture §8).
- Selecting a sidebar tag filters the grid to media carrying that path-qualified tag and its descendants (Architecture §3). Selecting a second tag reveals an AND/OR match-mode control; mode persists (Architecture §8, §10).

**Copy:** "Search Vesper" · "Sort: Date" · "Thumbnail size" · "Extra Small" · "Small" · "Medium" · "Large" · "Extra Large" · group title e.g. "July 2023" · "318 items".

**Acceptance:**

- Opening a media cell (activate/double-click/Enter) opens the viewer at that item (§5).
- Changing thumbnail size re-lays out the grid without a full reload and preserves the scroll anchor.
- Incremental index results arrive as batches; the grid never fully reloads or reorders per discovered file (Architecture §5).
- Search text is empty on every launch regardless of prior session.
- The interface stays responsive while scanning/thumbnailing runs in the background (Vision §1, Architecture §5).

---

## 3. Empty State — First Run / No Sources

Reference: `02_add_your_first_source.png`. Shown when no source roots exist (or none online with any indexed media). Architecture §9 `root_stack` page `empty`.

**Behavior:** The sidebar still renders, with a "No tags available" placeholder in the tag list and a "Tips" hint. The content area shows a centered illustration and the call to action.

**Copy (exact):**

- Headline: "Add folders to start browsing your media."
- Body: "Vesper indexes media from your folders so you can browse them beautifully and privately."
- Reassurance: "Nothing is moved or modified."
- Primary button: "Add Source Root"
- Link: "Learn more about source roots"
- Sidebar tag-list placeholder: "No tags available"
- Sidebar tip: "Add a source folder to start browsing your photos and videos."

> Note: the mockup renders "beatuifully"; ship the correct spelling "beautifully".

**Acceptance:**

- "Add Source Root" (in the content or sidebar footer) opens the system folder chooser (an allowed dialog, Vision §2). It never opens a custom modal.
- Adding the first valid root transitions `empty → grid` (crossfade) once basic records commit; the transition does not wait for thumbnails (Architecture §5).
- A rejected folder (nonexistent, not a directory, unreadable, non-canonicalizable, or overlapping an existing root) produces a recoverable inline Settings error, not a stored offline root and not a blocking dialog (Architecture §1).
- No indexing progress bar or modal appears at any point.

---

## 4. No-Results State — Filters Match Nothing

Reference: `03_no_matching_media.png`. Shown when at least one source root has visible media but the active search and/or tag filters match zero items. Architecture §9 `root_stack` page `no-results`. This is distinct from `empty` (§3).

**Behavior:** The header keeps the search entry (with its current text and a clear `✕`) and the sidebar keeps active tag selections. The content area shows a centered "no match" illustration and recovery actions. The Date/Type/Rating filter chips shown in the mockup are **not** built (see §0).

**Copy (exact):**

- Headline: "No media matches your filters."
- Body: "Try adjusting your search or removing folder tags and filters."
- Primary button: "Clear search"
- Link: "Review folder tags"

**Acceptance:**

- "Clear search" empties the search entry only; it does not clear tag filters. If clearing search yields matches, the page transitions `no-results → grid`.
- "Review folder tags" moves focus to the sidebar tag list (so the user can deselect tags). It opens no dialog.
- Deselecting the last active tag with empty search returns to `grid` (or `empty` if the library truly has no visible media).
- `no-results` never appears while the library has zero source roots — that is `empty` (§3).

---

## 5. Media Viewer

Reference: `04_media_viewer.png`. A full-window overlay (`viewer_overlay`) covering the sidebar and header and disabling their interaction (Architecture §9). Opening the viewer clears selection; viewer and selection mode are never active together (Architecture §9). Viewer open state is never persisted (Architecture §8).

**Behavior:**

- **Top bar:** brand + a context breadcrumb ("Library / July 2023 / 318 items"), and controls: info-panel toggle, fullscreen, overflow menu, close (`✕`). The mockup's in-viewer search/filter/grid/list buttons are not built (§0); the overflow menu carries "Open externally", "Reveal in Folder", and "Copy Path".
- **Stage:** the media fills the stage on a `graphite` background. A centered filename pill shows the filename and position, e.g. "IMG_2048.jpg" · "4 / 318". Left/right arrows (`‹` `›`) navigate within the current filtered/sorted result set. While a frame decodes, a stable placeholder shows; a decode failure shows a stable error placeholder, never a dialog (Vision §2, Architecture §5 generation guard).
- **Zoom controls:** fit-to-window, `−`, current level ("1:1"), `+`, and fullscreen. Zoom is direct (Visual §6). GIFs and videos: GIF shows first frame only; video plays inline (Vision §4).
- **Side panel** (`info_tags_panel`, toggleable) with two tabs, "Info" and "Tags", both **read-only**:
  - **Info** rows, in order: "File name", "Type", "Added", "Modified", "Dimensions", "Duration" (video only), "Folder", "Source" (full path). These derive from filesystem and application metadata only. EXIF rows (Date taken, Camera, Lens, ISO, Focal length, Aperture, Exposure), Location, and MD5/SHA256 shown in the mockup are **not** built (Vision §4, §5; Architecture §4).
  - **Tags**: the media's folder-lineage tags as read-only chips (e.g. the folders between the source root and the file). No "✕" remove and no "+ Add tag" — tags are folder-derived and cannot be edited (Vision §5, Architecture §3). A chip May act as a shortcut to filter the grid by that tag.

**Copy (exact):** tab labels "Info" · "Tags"; Info labels "File name" · "Type" · "Added" · "Modified" · "Dimensions" · "Duration" · "Folder" · "Source"; overflow items "Open externally" · "Reveal in Folder" · "Copy Path"; position format "N / M"; zoom label "1:1".

**Acceptance:**

- `‹`/`›` and Left/Right arrows move through the exact current result set; wrapping and bounds match the grid order.
- Closing the viewer (`✕` or Escape) returns to the grid at the originating scroll position and restores sidebar/header interaction.
- The Info panel shows no EXIF, no location, and no hash rows under any file type.
- The Tags tab exposes no way to add or remove a tag.
- A stale off-thread decode result for a superseded item is discarded (Architecture §5 generation guard); the viewer never shows the wrong image for the current position.

---

## 6. Selection Mode

Reference: `05_selection_mode.png`. Grid-scoped multi-select. Selection state is never persisted (Architecture §8) and is cleared when the viewer opens (Architecture §9).

**Behavior:**

- Entering selection (header select toggle, or Ctrl/Shift-click, or long-press) reveals the selection action bar (`action_bar_revealer`) sliding up over the grid. Selected cells show a 2px `lavender` ring and a `lavender` check badge top-right.
- The action bar shows, left to right: a count label "Selected N items", then "Open", "Reveal in Folder", "Copy Path", "Clear Selection". The mockup's "Add to Collection" is **not** built (Vision §5 limits batch actions to copy-path and open-location).
- The bottom status line reads "N items selected" (e.g. "5 items selected"), consistent with the grid's "318 items, 1 selected" idle form.

**Copy (exact):** "Selected 5 items" (pattern "Selected N items") · "Open" · "Reveal in Folder" · "Copy Path" · "Clear Selection" · status "5 items selected" · idle status "318 items, 1 selected".

**Acceptance:**

- "Open" opens the selected item(s) via the system handler; "Reveal in Folder" opens the file manager at the item's location; "Copy Path" places the file path(s) on the clipboard; "Clear Selection" empties the selection and hides the action bar.
- Clipboard preparation and file-manager launches run off the GTK thread (Vision §4 responsiveness budget).
- No batch action deletes, moves, renames, tags, rates, or collects media (Vision §5). The action bar exposes exactly the four allowed actions plus the count.
- Opening the viewer while items are selected clears the selection first.

---

## 7. Offline Source Root

Reference: `07_source_root_offline.png`. Shown when one or more source roots are unavailable at launch or disappear while running (Architecture §1). Priority 2 in the `status_banner_stack`.

**Behavior:** A full-width banner appears below the header with a `warning` info glyph, the message, a "Details" link, and a close control. The offline root's media is hidden from the grid, search, selection, viewer navigation, and tag counts, but its records are preserved; the root stays listed in Settings with a passive offline marker. No blocking dialog appears. Filters that reference an offline root's tag are suspended (not discarded) and the banner explains this (Architecture §8).

**Copy (exact):** "1 source root is currently unavailable. Existing indexed items remain browsable." · "Details". The count updates for multiple roots (pattern "N source roots are currently unavailable. …").

**Acceptance:**

- The banner is passive and dismissible; dismissing it does not bring the root back online and does not clear the suspended-filter state.
- "Details" surfaces which roots are offline (in the banner's detail affordance or Settings), not a modal error.
- When an offline root returns, it is rescanned before its media re-enters visible results and tag counts; suspended filters that referenced it are restored after a successful rescan (Architecture §1, §8).
- Offline media never renders as a visible placeholder cell in the grid (Vision §5).

---

## 8. Scan Issues

Reference: `08_scan_issues.png`. Aggregate, passive indicator for unreadable/inaccessible supported files. Reached via the bottom-left scan-issue button (`scan_error_button`), independent of the status banner (Architecture §9, §10).

**Behavior:** A small pill sits at the bottom-left of the grid with a `warning` glyph and label; activating it opens a `surface` popover with a short explanation and a "Details" affordance. Scan errors are tracked per path plus scan generation; a later successful scan of the same path clears its error (Architecture §5). Scanner-level temporary files (`.tmp`, `.part`, `.crdownload`, `.swp`, `~`, etc.) never produce scan errors (Architecture §6).

**Copy (exact):** pill and popover title "Some files could not be scanned" · popover body "Vesper couldn't access a few files or folders. They may be offline, unsupported, or require permissions." · button "Details".

**Acceptance:**

- The indicator is passive: it never blocks browsing and never raises a per-file dialog (Vision §2).
- It remains reachable even while the offline banner (higher priority) is shown.
- When all tracked scan errors clear, the indicator disappears.
- Unsupported and ignored files produce no indicator at all — only unreadable *supported* files do (Vision §2).

---

## 9. Settings

Reference: `06_settings.png`. Settings is an allowed dialog/panel exception (Vision §2). It has a left navigation and a content pane.

**Navigation (exact, in order):** "Source Roots", "Ignore Rules", "Appearance", "Thumbnails", "Advanced", "About Vesper". The mockup's "Metadata" page is **not** built (no EXIF in v1, §0); the root-as-tag toggle lives under "Source Roots".

**Source Roots pane:**

- Title "Source Roots"; description "Folders and drives that Vesper monitors for media."; actions "Add Source Root" and "Remove".
- A table with columns "Location" (path + device), "Type" (e.g. "Internal Drive", "USB Drive", "Network Share", "External Drive"), and "Items" (indexed count).
- Footer: "Changes are applied automatically. You can add, remove, or reorder roots at any time."
- The root-as-tag toggle (default OFF) appears here; toggling re-derives all tags (Architecture §3).

**Ignore Rules pane:**

- Title "Ignore Rules"; description "Patterns for files and folders that Vesper should skip."; actions "Add Rule" and "Reset to Defaults".
- Presented as rows with columns "Pattern", "Description", "Examples". The underlying model is a pattern list, one pattern per line, evaluated per Architecture §2 (global rules first, last match wins). The table is a presentation of that list; it does not change matching semantics.
- Footer: "Patterns are matched using glob syntax. Use * for wildcards and / to match folders." with a "View glob reference" link.
- The default pattern set is exactly Architecture §2's list (`.git/`, `node_modules/`, `.Trash/`, `.cache/`, `*.tmp`, `*.part`, `.DS_Store`, `Thumbs.db`). Vesper does **not** ship a default that ignores all hidden files (`.*`) — hidden files are indexed unless a rule excludes them (Architecture §1). "Reset to Defaults" is an explicit, user-initiated action; defaults are never restored automatically (Architecture §2).

**Copy (exact):** "Settings" · "Source Roots" · "Ignore Rules" · "Appearance" · "Thumbnails" · "Advanced" · "About Vesper" · "Folders and drives that Vesper monitors for media." · "Add Source Root" · "Remove" · "Location" · "Type" · "Items" · "Changes are applied automatically. You can add, remove, or reorder roots at any time." · "Patterns for files and folders that Vesper should skip." · "Add Rule" · "Reset to Defaults" · "Pattern" · "Description" · "Examples" · "Patterns are matched using glob syntax. Use * for wildcards and / to match folders." · "View glob reference" · "Source roots define where Vesper looks for your media. Ignore rules let you skip files and folders you don't want to include." · "Learn more".

**Acceptance:**

- Adding a root uses the system folder chooser; a rejected root shows a recoverable inline error and is not stored (Architecture §1).
- Applying valid global ignore rules triggers a rescan of all online roots; an invalid pattern keeps the previous saved rules and identifies the offending line, without partial application (Architecture §2).
- "Reset to Defaults" replaces the current rules with the §2 default set only when the user invokes it.
- Only one index-mutating maintenance operation runs at a time; starting another shows a passive "Library maintenance is already running" status, not overlapping work (Architecture §5).

---

## 10. Sidebar Collapse

Not a separate screen — a shell behavior visible across `05`, `07`, `08`. The sidebar collapses fully to give media more room (Architecture §9 `sidebar_revealer`).

**Behavior:** A collapse control (`«`) in the sidebar brand block hides the sidebar (slide, Visual §6); a restore toggle (`☰`) appears at the header start and brings it back. When collapsed, the grid area expands to full width. The collapsed state persists across launches (Architecture §8).

**Acceptance:**

- Collapsing/expanding animates within 200ms and instantly when reduced motion is active (Visual §6).
- The collapsed state is restored on the next launch.
- Collapsing never resizes the sidebar to a partial rail — it is either the fixed 220px or fully hidden (Architecture §9). There is no drag-to-resize.

---

## Cross-References

- [Vision — Non-Goals & Rejected Features](01_Vision.md#5-explicitly-rejected-features)
- [Architecture §1 — Source Directory Model](02_Architecture.md#1-source-directory-model)
- [Architecture §8 — Session Persistence](02_Architecture.md#8-session-persistence-behavior)
- [Architecture §9 — Widget Tree](02_Architecture.md#9-widget-tree-source-of-truth)
- [Architecture §10 — State → UI Mapping](02_Architecture.md#10-state--ui-mapping)
- [Implementation](03_Implementation.md)
- [Visual Design](05_Visual_Design.md)
