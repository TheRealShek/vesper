# Visual Design

---

## 0. How to Read This Document

This document owns the visual language of Vesper v1: color, type, spacing, iconography, elevation, and motion. It is subordinate to [01_Vision.md](01_Vision.md) and [02_Architecture.md](02_Architecture.md). Where a mockup shows a feature the Vision rejects (EXIF panels, star ratings, GPS/location, content hashes, manual tags, collections), this document does **not** style it — the mockups are a visual reference, not a scope authority.

`Must`/`must not` are requirements. `Should` is a strong default. `May` is optional.

Dark is the canonical theme. Vesper follows the system light/dark preference and falls back to dark when no preference is available (Vision §7). Every token below has a dark (canonical) and a light value; the dark value is authoritative when the two disagree.

**Guiding rule:** Media is the product; the frame recedes. Chrome is low-chroma graphite and slate. Saturated color (indigo/lavender) appears only on the primary action, the active navigation row, the current selection, and focus — never as decoration behind or beside media.

---

## 1. Color Tokens

All UI color resolves to one of these named tokens. Raw hex values must not appear in widget code or CSS outside the token definitions.

| Token           | Dark (canonical) | Light     | Role                                                                    |
| --------------- | ---------------- | --------- | ----------------------------------------------------------------------- |
| `graphite`      | `#0F1115`        | `#FAFAFB` | App window background; deepest layer; grid canvas behind media          |
| `slate`         | `#171A20`        | `#F2F3F5` | Sidebar panel; secondary surfaces adjacent to the canvas                |
| `surface`       | `#1F232B`        | `#FFFFFF` | Elevated surfaces: popovers, header controls, cards, settings rows      |
| `surface-raise` | `#262B34`        | `#ECEEF1` | Hover/pressed state of a `surface`; selection-bar background            |
| `indigo`        | `#5B5BEF`        | `#4F4FE0` | Primary accent: primary button, active nav row fill, active menu item   |
| `indigo-hover`  | `#6E6EF2`        | `#6161E8` | Hover state of `indigo`                                                  |
| `lavender`      | `#9B9DF2`        | `#6B6EEA` | Secondary accent: text links, selection ring, focus ring, active glyphs |
| `text`          | `#ECEDEF`        | `#1A1C20` | Primary text and active icons                                           |
| `muted`         | `#9298A2`        | `#5C626C` | Secondary text, counts, metadata labels, resting icons                  |
| `border`        | `#2A2F38`        | `#DDE0E5` | Hairline separators, control outlines, cell borders                     |
| `warning`       | `#E0A94A`        | `#B77A12` | Offline banner and scan-issue accents only                              |
| `scrim`         | `rgba(0,0,0,.55)`| `rgba(0,0,0,.45)` | Translucent overlays: video-duration badge, viewer filename pill |

**Usage rules:**

- `indigo` is reserved for exactly one primary action per surface and for the single active navigation row. Two indigo primary buttons must never be visible at once.
- `lavender` carries links, the selection ring around a chosen cell, and the keyboard focus ring. It must not be used as a fill.
- `warning` is the only non-neutral, non-accent color, and appears only in the offline banner (§7 Product Spec) and the scan-issue indicator (§8 Product Spec). It must never signal a per-file failure inline in the grid.
- The three-layer depth model is `graphite` (canvas) → `slate` (sidebar) → `surface` (popovers/cards). Depth is expressed by these fills plus a single `border` hairline, not by shadows. A soft shadow (`0 1px 3px scrim`) May be used on popovers and the viewer chrome only.

---

## 2. Typography

Typeface: **Inter** (variable). Fallback: system UI sans (`Cantarell`, `-apple-system`, `sans-serif`). Numerals in counts, dimensions, durations, and positions use Inter's tabular figures (`font-feature-settings: "tnum"`).

| Token         | Size / Line | Weight | Tracking | Use                                                              |
| ------------- | ----------- | ------ | -------- | --------------------------------------------------------------- |
| `display`     | 30 / 38     | 600    | −0.01em  | Empty-state and no-results headline                             |
| `title-1`     | 20 / 28     | 600    | normal   | App name "Vesper"; viewer brand                                 |
| `title-2`     | 18 / 26     | 600    | normal   | Grid group header ("July 2023"); settings section title         |
| `body-strong` | 14 / 20     | 500    | normal   | Sidebar tag label; button label; tag chip; active row           |
| `body`        | 14 / 20     | 400    | normal   | Metadata values; empty/no-results paragraphs; descriptions      |
| `caption`     | 13 / 18     | 400    | normal   | Item counts; metadata labels; secondary/subtitle text           |
| `overline`    | 12 / 16     | 600    | 0.04em   | Uppercase group labels (settings nav); small tracked labels     |
| `badge`       | 12 / 16     | 600    | normal   | Video-duration badge; viewer position ("4 / 318")               |

**Rules:**

- Only one `display` heading may appear on screen (empty **or** no-results, never both).
- The app subtitle ("Quiet nocturne media gallery" / "Quiet nocturne / GNOME-native media gallery") is `caption` in `muted`.
- File paths (viewer Source row, settings Location column) render in `body` with tabular figures; a monospace face May be used but is not required.
- Never bold for emphasis inside a paragraph; use `muted`→`text` contrast instead.

---

## 3. Spacing & Layout

Base unit is **8px** with a **4px** half-step. The allowed scale is the only set of spacing values used:

```
4  8  12  16  24  32  48
```

| Value | Name  | Typical use                                                        |
| ----- | ----- | ------------------------------------------------------------------ |
| 4     | `xs`  | Icon-to-label gap; chip inner padding (vertical)                   |
| 8     | `sm`  | Control inner padding; count-to-row-edge gap                       |
| 12    | `md`  | **Grid outer padding and inter-cell gutter** (uniform, per VIS-12) |
| 16    | `lg`  | Section padding; sidebar row height padding; card inset            |
| 24    | `xl`  | Between grouped blocks (settings sections; empty-state stack)      |
| 32    | `2xl` | Empty-state vertical rhythm around the illustration                |
| 48    | `3xl` | Large empty-region breathing room                                  |

**Fixed dimensions:**

- Sidebar: fixed **220px** wide when expanded (enforced in CSS as `min-width == max-width`; no `width_request` in Rust, no `GtkPaned`). Collapses fully via `sidebar_revealer` (Architecture §9).
- Header bar: **52px** tall.
- Sidebar tag row: **40px** tall.
- Grid: `12px` outer padding on all sides and `12px` gutter between cells, at every thumbnail size. The five thumbnail sizes change cell dimensions, never the gutter.
- Cell corner radius `8px`; button/chip radius `8px`; popover/card radius `12px`.

---

## 4. Iconography

- Style: **GNOME symbolic** icons — single-path, monochrome, 16px nominal (20px in the header, 24px only in the empty-state affordances). Prefer named Adwaita symbolics; custom glyphs must match their stroke weight and optical size.
- Tint: icons inherit `currentColor`. Resting `muted`; hover/focus `text`; **active `lavender`, or `text` on an `indigo` fill**. The accent tint is applied sparingly — active nav glyph, primary-action glyph — never to a whole toolbar at once.
- The video-duration badge (`0:14`) is a filled white play triangle plus `badge` text on a `scrim` pill, bottom-left of the cell.
- Source-root type glyphs (Internal Drive, USB Drive, Network Share, External Drive — settings §9 Product Spec) are symbolic and monochrome; the type is conveyed by glyph, not color.
- Sidebar tag rows show a leading folder symbolic; the app never invents category-specific icons per tag name.

---

## 5. Elevation & Surfaces

Depth is fill + hairline, not shadow:

1. `graphite` — the window canvas and the grid background behind media.
2. `slate` — the sidebar, separated from the canvas by one `border` hairline.
3. `surface` — popovers (thumbnail-size, sort, primary menu), settings rows/cards, header controls; `surface-raise` on hover.

- Separators are a single 1px `border` line. No double borders, no inner glows.
- The viewer chrome (top bar, filename pill, nav arrows, zoom controls, side panel) floats over media on `scrim`/`surface` with an optional soft shadow; the media surface itself sits on `graphite`.
- The selection ring is a 2px `lavender` border inset on the cell plus a filled `lavender` check badge, top-right (Product Spec §6).

---

## 6. Motion & Transitions

Motion clarifies state change and never competes with media. All durations are `≤ 250ms`. Enters use ease-out; exits use ease-in; state swaps use crossfade.

| State change                                | Property                      | Duration | Curve     | Widget / mechanism                          |
| ------------------------------------------- | ----------------------------- | -------- | --------- | ------------------------------------------- |
| **empty ↔ no-results ↔ grid**               | crossfade                     | 200ms    | ease      | `root_stack` (`GtkStack` CROSSFADE)         |
| **viewer open**                             | opacity 0→1, scale .98→1      | 220ms    | ease-out  | `viewer_overlay`                            |
| **viewer close**                            | opacity 1→0, scale 1→.98      | 180ms    | ease-in   | `viewer_overlay`                            |
| **selection mode on/off**                   | slide/reveal up               | 200ms    | ease-out  | `action_bar_revealer` (`GtkRevealer`)       |
| **sidebar collapse/expand**                 | slide horizontal              | 200ms    | ease-out  | `sidebar_revealer` (`GtkRevealer` SLIDE)    |
| **status banner in/out** (offline, scan)    | reveal down                   | 200ms    | ease-out  | `status_banner_stack`                       |
| **thumbnail size change**                   | cell relayout                 | 150ms    | ease      | `grid_view` re-measure                      |
| **selection ring / check appear**           | opacity + scale .9→1          | 120ms    | ease-out  | `.media-cell.selected`                      |
| **hover** (rows, cells, buttons)            | background / opacity          | 100ms    | ease      | any interactive surface                     |
| **viewer prev/next**                        | crossfade of media surface    | 150ms    | ease      | `viewer_stage`                              |

**Rules:**

- The grid never animates per-cell on scroll or on incremental index results; thumbnails fade in individually at `120ms` when their image resolves, and only then.
- Media content is never animated by the frame: GIFs show the first frame only (Vision §4); zoom in the viewer is direct, not spring-based.
- Respect reduced motion: when `gtk-enable-animations` is false or the platform reports `prefers-reduced-motion`, every transition above collapses to an instant state swap (duration 0, crossfade → none). No functionality depends on a transition completing.
- Loading and error states inside a cell or the viewer use a static placeholder, not a spinner-driven layout shift (Architecture §5 publication order; the first grid appears from basic records, not thumbnails).

---

## 7. Component Visual Reference

Concise visual specs; behavior lives in [04_Product_Spec.md](04_Product_Spec.md) and structure in [03_Implementation.md](03_Implementation.md).

- **Primary button** (`Add Source Root`, `Clear search`): `indigo` fill, `text`(white) label, `8px` radius, `indigo-hover` on hover. One per surface.
- **Text link** (`Learn more about source roots`, `Review folder tags`, `Details`): `lavender`, no underline until hover.
- **Sidebar tag row**: `body-strong` label left, `caption` `muted` count right, `40px` tall. Active row: `indigo` fill wash + `text` label + `lavender` leading glyph. Hover: `surface-raise`.
- **Media cell**: `8px` radius, no border at rest; `border` hairline on hover with a top-left check affordance; `.selected` → 2px `lavender` inset ring + `lavender` check badge top-right.
- **Video badge**: `scrim` pill, white triangle + `badge` duration, bottom-left, `8px` from edges.
- **Status banner (offline)**: full-width row below header, `slate` fill, `warning` left icon + `body` text + `Details` link + close, `warning` hairline.
- **Scan-issue indicator**: bottom-left pill on the grid, `warning` outline, warning glyph + short label + close; opens a `surface` popover.
- **Viewer filename pill**: centered top, `scrim` background, `body-strong` filename + `muted` `badge` position ("4 / 318").
- **Tag chip** (viewer Tags tab, read-only): `surface` fill, `border` hairline, `body-strong` label, `8px` radius. No remove control, no add control (Vision §5 — tags are folder-derived).
- **Focus ring**: 2px `lavender` outline with 2px offset on every keyboard-focusable widget.

---

## Cross-References

- [Vision — Philosophy & Non-Goals](01_Vision.md#2-core-philosophy-and-non-goals)
- [Architecture §9 — Widget Tree](02_Architecture.md#9-widget-tree-source-of-truth)
- [Architecture §10 — State → UI Mapping](02_Architecture.md#10-state--ui-mapping)
- [Product Spec](04_Product_Spec.md)
- [Implementation](03_Implementation.md)
