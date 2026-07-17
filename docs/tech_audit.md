# Vesper Codebase Audit & Technical Debt

**Originally created:** 2026-07-15
**Last verified:** 2026-07-17 (fresh from-scratch re-audit against the five specs)
**Resolved:** 2026-07-17 (all issues below fixed; validation gate green)

This document records a from-scratch verification audit of Vesper against the
five numbered specification documents:
`01_Vision.md`, `02_Architecture.md`, `03_Implementation.md`,
`04_Product_Spec.md`, and `05_Visual_Design.md`.

The previous cycle (commit `3304dbe`) closed the entire A/B/I/T/U/V/NEW/ARCH
backlog, and those fixes remain in place and verified this pass — path-qualified
tags, the migration runner, the 8-tier search ranking, the 300ms debounce, the
SQLite-backed settings/session state, the library lock, generation-tagged
query/hydration chunks, the liveness worker, the symlink/canonical-identity
walker, the double-metadata + 1s/5s/30s stability check, and the async
maintenance operations all still match their specs.

This re-audit surfaced a new set of issues, concentrated in the visual layer
(`05_Visual_Design.md`) and a handful of viewer/keyboard behaviors
(`04_Product_Spec.md`). None were data-loss or crash defects; most were visual
contract violations and missing keyboard affordances.

**All of them are now fixed** (VIS-1…11, VWR-1…4, KEY-1/2, GRID-1, COPY-1,
THEME-1), applied in the single-concern fix order below. The validation gate
passes (109 tests green) and the app starts cleanly with no CSS-parse warnings.
Each issue is retained below as a record, with its fix noted at the end of the
entry.

Validation gate (run after any `.rs` change; documentation-only edits are exempt):

```text
cargo fmt --check && cargo clippy -- -D warnings && cargo test
```

## How to read this document

- Every row in the backlog below is **RESOLVED** as of 2026-07-17.
- Each issue cites the owning spec section, the spec-vs-code delta, a
  `file:line` anchor, and a **Fixed:** note describing the change.
- **Severity:** `High` = user-visible behavior/acceptance gap; `Med` = clear
  visual-contract or responsiveness violation; `Low` = cosmetic/cleanup.

---

# Resolved issues

## A. Visual Design contract (`05_Visual_Design.md`, `03_Implementation.md` §3 CSS)

### VIS-1 — Sidebar section labels are spaced all-caps · Med
Spec 05 §3 and §5 (and the 03 §1 tree) require sentence-case `Tags` and
`Sources`. Code sets the literal labels `"TAGS"` and `"SOURCES"`, exactly the
"spaced all-caps labels" 05 §3 forbids and 05 §10 lists for removal.
`src/ui/sidebar.rs:190` (`"TAGS"`), `src/ui/sidebar.rs:356` (`"SOURCES"`).
**Fixed:** labels are now sentence-case `Tags` / `Sources`.

### VIS-2 — Source list is still wrapped in a card · Med
Spec 05 §5 ("Remove the card around the source list") and 05 §10 ("source-list
card") require flat source rows. The sources list is wrapped in a `gtk::Frame`
with `css_classes(["card", "sources-card"])`, and `.sources-card` styling
persists in CSS. `src/ui/sidebar.rs:367-373`; `src/ui/style.css:165-179`.
**Fixed:** the `gtk::Frame` card is gone; the list is appended directly with a
flat `.sources-list` class (row/hover styling preserved, card removed).

### VIS-3 — Decorative letter-spacing on the hover filename overlay · Med
Spec 05 §3 forbids decorative letter spacing on filenames; 05 §10 lists
"letter-spaced labels" for removal. `.card .cell-hover-overlay` sets
`letter-spacing: 0.05em`. `src/ui/style.css:34`.
**Fixed:** the `letter-spacing` declaration was removed.

### VIS-4 — Hover filename gradient wrong · Med
Spec 05 §2 / 03 §3 CSS: black **82%** at the bottom, transparent by ~**60%**
height (`linear-gradient(to top, rgba(0,0,0,0.82), transparent 60%)`). Code uses
`rgba(0,0,0,0.75)` with a plain `transparent` stop (no 60% anchor).
`src/ui/style.css:30`.
**Fixed:** gradient is now `rgba(0,0,0,0.82)` → `transparent 60%`.

### VIS-5 — Viewer scrim opacity is 85%, spec is 92% · Med
Spec 04 §8 / 05 §8 / 03 §3 CSS all require a **92%** black scrim
(`.viewer-bg { rgba(0,0,0,0.92) }`). Code uses `rgba(0,0,0,0.85)`.
`src/ui/style.css:121`.
**Fixed:** `.viewer-bg` is now `rgba(0,0,0,0.92)`.

### VIS-6 — Duration badge opacity is 60%, spec is 78% · Med
Spec 05 §2 opacity table: duration badge is black at **78%**. Code uses
`rgba(0,0,0,0.6)` (and `font-weight: bold`, where 05 §6 asks for normal/medium
numeric weight). `src/ui/style.css:56-63`.
**Fixed:** badge is now `rgba(0,0,0,0.78)` with `font-weight: 500`.

### VIS-7 — Video-control OSD surface below the 78–84% floor · Low
Spec 05 §2 puts the viewer OSD surface at black **78–84%**. `.video-controls`
uses `rgba(0,0,0,0.75)`. `src/ui/style.css:130`.
**Fixed:** `.video-controls` is now `rgba(0,0,0,0.82)`.

### VIS-8 — Grid gap is ~24px, spec is 12px · Med
Spec 05 §2/§6 (and 03 §3 CSS) fix the visible media-to-media gap at **12px**,
composed as 4px `border-spacing` + 4px cell margins. Code sets
`gridview { border-spacing: 16px }` with 4px card margins → ~24px gap. The
`GRID_ROW_SPACING = 16` constant and the stale "8px border-spacing" comment in
`window.rs` are tied to this same wrong value. `src/ui/style.css:206`;
`src/ui/window.rs:13`, `src/ui/window.rs:969-971`.
**Fixed:** `border-spacing` is now `4px` (→ 12px visible gap with the 4px cell
margins); `GRID_ROW_SPACING` is `12` (the true stride gap) with an accurate
doc-comment, and the stale box-shadow/8px margin comment was rewritten.

### VIS-9 — Hard-coded white surfaces break the light theme · Med
Spec 05 §2 forbids hard-coded application surfaces and requires theme-named
colors; 03 §3 CSS specifies `alpha(@window_fg_color, 0.12)`. Several rules
hard-code `rgba(255,255,255,·)`, which will not adapt in light mode: sidebar
border-right `rgba(255,255,255,0.07)` (`src/ui/style.css:198`), sidebar
separator `rgba(255,255,255,0.12)` (`:215`), source-row hover
`rgba(255,255,255,0.04)` (`:173`). Also the sidebar panel uses
`@window_bg_color` where 03 §3 CSS specifies `@headerbar_bg_color`
(`src/ui/style.css:197`).
**Fixed:** all three whites are now `alpha(@window_fg_color, ·)` (border-right
and separator at 0.12 per 03 §3, source-row hover at 0.04), and the sidebar
panel background is `@headerbar_bg_color`.

### VIS-10 — Viewer error/unavailable icon is 96px, max is 48px · Med
Spec 05 §8 caps error-state icons at **48px** and 04 §4 explicitly says "Do not
enlarge an error/empty-state icon to 96px." The viewer error icon is built with
`.pixel_size(96)`. `src/ui/viewer.rs:157-160`.
**Fixed:** the error icon is now `.pixel_size(48)`.

### VIS-11 — Dead `.top-header` gradient CSS · Low
05 §8 forbids a decorative top-header gradient; the `.top-header` rule remains in
CSS but is referenced nowhere in Rust (leftover from a removed treatment).
`src/ui/style.css:136-138`.
**Fixed:** the dead `.top-header` rule was deleted.

## B. Viewer behavior (`04_Product_Spec.md` §6/§8, `05_Visual_Design.md` §8)

### VWR-1 — Close/Info are separate circular buttons, not one OSD toolbar · Med
Spec 04 §8 and 05 §8/§10: "Close and Info form one compact top-right OSD
toolbar … they are not separate oversized circular buttons." Code builds
`close_btn` and `info_btn` as independent `circular osd` overlays positioned by
margin. `src/ui/viewer.rs:198-216`, `src/ui/viewer.rs:354-355`.
**Fixed:** Info and Close are now flat 44px buttons inside one
`.viewer-osd-toolbar` box (8px radius/spacing), added as a single overlay; the
box is hidden as a unit in fullscreen.

### VWR-2 — Image decode failure shows no message · High
Spec 04 §6: on decode failure the viewer must show **"This image could not be
displayed."** while close/next/prev stay functional. On a failed
`Texture::from_bytes`, code silently leaves the picture blank and sets
dimensions to `"Unknown"` — no error surface. `src/ui/viewer.rs:908-918`.
**Fixed:** a decode failure now sets the error label to "This image could not
be displayed." and switches to the error stack page; nav/close stay live.

### VWR-3 — Fullscreen not exited on close; Escape skips the fullscreen tier · High
Spec 04 §22 Escape precedence: fullscreen exits **before** the viewer closes.
Escape is not handled in the viewer's own key controller, so it falls through to
handlers that call `close()` directly — closing the whole viewer from
fullscreen. `close()` never calls `unfullscreen()`, so the GTK window is left in
OS fullscreen showing the grid. `src/ui/viewer.rs:670-713`;
`src/ui/selection_bar.rs:158-162`; `src/ui/window.rs:1386-1389`.
**Fixed:** a single `Viewer::handle_escape()` implements the precedence
(fullscreen exits first, else close) and is called from the viewer's own
capture controller and both external Escape handlers; `close()` now
defensively `unfullscreen()`s so the toplevel is never left fullscreen.

### VWR-4 — Full-size image decode runs on the GTK thread · Med
Spec 03 §6/§9 and 04 §15: full-size reads **and decode** run off the GTK
thread; only texture install happens on it. The viewer awaits
`load_contents_future()` (off-thread read) but then calls
`gtk::gdk::Texture::from_bytes(...)` — a synchronous decode — inside the
GTK-thread future, which can exceed the 8ms input budget on large images.
`src/ui/viewer.rs:908-918`.
**Fixed:** read + decode now run in `tokio::task::spawn_blocking` (via the
`image` crate) awaited from the local future; only the cheap `MemoryTexture`
build/install happens on the GTK thread.

## C. Selection & keyboard (`04_Product_Spec.md` §9/§16/§22)

### KEY-1 — `Ctrl+Space` / `Shift+Space` selection keys missing · High
Spec 04 §9, §16, and §22 require `Ctrl+Space` (toggle focused cell) and
`Shift+Space` (range-select from anchor to focused cell) as keyboard equivalents
of modifier-click. The grid key handler implements only `Escape` and `Ctrl+A`;
`Key::space` is handled only inside the viewer (play/pause). No grid Space
handling exists. `src/ui/selection_bar.rs:157-190`; `src/ui/viewer.rs:562`.
**Fixed:** the grid key handler now handles `Ctrl+Space` (toggle) and
`Shift+Space` (range from anchor), mirroring the modifier-click history/anchor
logic. The focused cell's model position is tracked via an
`EventControllerFocus` on each cell into a shared `focused_position` cell;
grid Space is suppressed while the viewer is open.

### KEY-2 — Opening the viewer via Enter does not clear selection · Low
Spec 04 §8/§9: opening the viewer clears the active selection and hides the
action bar (the mouse path in `grid_cell.rs` does this). The `connect_activate`
(Enter) path calls `viewer.open(pos)` without clearing the selection model, so
the selection/action bar persist under the viewer. `src/ui/window.rs:1182-1184`.
**Fixed:** `connect_activate` now clears the selection model, history, and
anchor before `open(pos)`, matching the mouse path.

## D. Grid cell (`04_Product_Spec.md` §5/§7)

### GRID-1 — Empty duration badge shown for unknown-duration video · Low
Spec 04 §5/§7: when duration is unavailable, **no** badge is shown. For any
video the badge is forced visible; when `duration < 0` the text is cleared but
`set_visible(true)` still runs, leaving an empty dark badge rectangle.
`src/ui/grid_cell.rs:484-491`.
**Fixed:** the badge is shown only when `duration >= 0`, in both the bind path
and the `duration-secs` notify handler (so it appears when a duration arrives).

## E. Copy & empty states (`04_Product_Spec.md` §4/§20)

### COPY-1 — No-results copy does not match spec · Low
Spec 04 §4/§20: the no-results page reads **"No media matches the current
filters."** with a **"Clear filters"** button. Code uses title `"No Results"`,
description `"Try a different search or tag combination."`, and button
`"Clear All Filters"`. `src/ui/window.rs:1159-1164`, `src/ui/window.rs:357-360`.
**Fixed:** the page title is now "No media matches the current filters." (no
description) and the button reads "Clear filters".

## F. Theme (`01_Vision.md` §4, `04_Product_Spec.md` §16)

### THEME-1 — No explicit dark fallback when no system preference exists · Low
Spec 01 §4 / 04 §16: follow the system dark/light preference, and **default to
dark** when none is available. There is no `adw::StyleManager` color-scheme
configuration anywhere; the app relies entirely on libadwaita's system-follow
default, so the specified dark fallback is not asserted.
(Absent — expected near app startup in `src/main.rs`.)
**Fixed:** `connect_startup` now sets
`adw::StyleManager::default().set_color_scheme(adw::ColorScheme::PreferDark)`,
which follows an explicit light system preference but defaults to dark
otherwise.

---

# Fix order (applied)

Grouped to keep each change reviewable and single-concern, per 05 §11. Each
group below was applied as its own pass:

1. **CSS token pass** (VIS-3, VIS-4, VIS-5, VIS-6, VIS-7, VIS-8, VIS-9, VIS-11):
   correct opacities, gradient stops, grid spacing, and swap hard-coded whites
   for theme-named colors in one `style.css` sweep; verify light/dark startup.
2. **Sidebar pass** (VIS-1, VIS-2): sentence-case labels, drop the source-list
   card/frame.
3. **Viewer pass** (VWR-1, VWR-2, VWR-3, VWR-4, VIS-10): combined OSD toolbar,
   image-decode error surface, fullscreen-exit-on-Escape + unfullscreen on
   close, off-thread decode, 48px error icon.
4. **Keyboard/selection pass** (KEY-1, KEY-2): add `Ctrl+Space` / `Shift+Space`;
   clear selection on Enter-activate.
5. **Copy & cell polish** (GRID-1, COPY-1): hide empty duration badges; align
   no-results copy.
6. **Theme fallback** (THEME-1): set the dark default via `adw::StyleManager`.

Every step passed the validation gate (`cargo fmt --check && cargo clippy -- -D
warnings && cargo test`, 109 tests green) and none combined visual cleanup with
backend/schema changes.
