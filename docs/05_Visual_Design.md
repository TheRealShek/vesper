# Visual Design

---

## 0. Ownership and Intent

This document is the source of truth for Vesper's visual language: hierarchy, color, opacity, spacing, radii, icons, motion, and the visual treatment of every primary surface. [04_Product_Spec.md](04_Product_Spec.md) owns behavior and [03_Implementation.md](03_Implementation.md) owns GTK structure. If a CSS example in another document conflicts with this document, this document wins.

Vesper should look like a focused GNOME media application, not a generated dashboard. The media supplies the color and personality. Application chrome is quiet, neutral, native, and deliberately sparse.

---

## 1. Visual Direction

The target character is **quiet, editorial, and native**:

- **Quiet:** controls are obvious when needed but do not compete with thumbnails.
- **Editorial:** alignment, spacing, and typography create hierarchy; decoration does not.
- **Native:** use libadwaita components, theme colors, symbolic icons, focus behavior, and disabled states before adding custom CSS.
- **Media-first:** thumbnails are not placed inside elevated dashboard cards. The grid should read as one gallery surface.
- **Honest:** loading, selection, offline state, and errors are explicit. Low opacity is never used to make important controls look “sleek.”

The design must avoid the common generated-interface pattern of rounding every surface, adding a shadow to every item, using pills for ordinary rows, sprinkling icons beside self-explanatory text, and layering several translucent gradients.

---

## 2. Foundation Tokens

Use these values consistently. Rust-visible values belong in `src/config.rs`; CSS-only values may be defined once at the top of `style.css`.

### Spacing

Use the 4px rhythm for spacing: `4`, `8`, `12`, `16`, `24`, `32`. Structural strokes and focus allowances may use 1–3px. Do not introduce spacing values such as 10, 14, 18, or 22 without a documented layout reason.

| Use | Value |
| --- | ---: |
| Icon-to-label gap | 6–8px |
| Compact control gap | 8px |
| Grid gap | 12px |
| Sidebar horizontal inset | 12px |
| Header control gap | 8px |
| Viewer safe edge | 24px |

### Radius

| Surface | Radius |
| --- | ---: |
| Media thumbnail | 6px |
| Custom compact row | 6px |
| Viewer OSD/control strip | 8px |
| Badges | 4px |
| Popovers/dialogs/buttons | Native libadwaita value |

Do not use a large radius to signal quality. Ordinary list rows are not pills. Circular shape is reserved for a check indicator or a genuinely circular native control.

### Color

- Do not redefine `accent_color` or `accent_bg_color`; inherit the user's system accent.
- Use libadwaita named colors such as `@window_bg_color`, `@view_bg_color`, `@headerbar_bg_color`, `@card_bg_color`, `@window_fg_color`, `@borders`, and the native accent colors.
- Do not hard-code dark application surfaces such as `#181818` or `#242424`.
- Do not tint the whole interface blue, purple, or gray to manufacture a brand. Vesper's identity comes from composition and media, not a custom dashboard palette.
- Accent color communicates focus, selection, or a suggested primary action only. It is not a decoration for every tag, badge, and button.

### Opacity and contrast

Opacity has a semantic purpose and is not a general styling tool:

| Element | Treatment |
| --- | --- |
| Primary controls and text | Fully opaque |
| Secondary text | Native `.dim-label`; do not add another opacity |
| Disabled controls | Native GTK disabled state |
| Selected thumbnail tint | Black at 12% maximum |
| Duration badge | Black at 78%, white text |
| Hover filename gradient | Black at 82% at bottom, transparent by roughly 60% height |
| Viewer scrim | Black at 92% in windowed viewer; 100% in fullscreen |
| Viewer OSD surface | Black at 78–84%, fully opaque icons/text |

Interactive controls must never rest at 40–60% opacity. Hover may change the surface, not rescue an illegible icon. Offline source rows retain readable text and show an explicit `Offline` label; do not dim the entire row below usable contrast.

### Elevation

- Grid cells, sidebar rows, tag rows, and source rows have no drop shadow.
- Native popovers, menus, and dialogs keep native elevation.
- The selection bar is edge-attached and separated by a border, not a floating shadowed capsule.
- The viewer uses a scrim and OSD surfaces; it does not need a scale transform or a shadow around the media.

---

## 3. Typography

- Use the system font and libadwaita type classes. Do not ship a display font for v1.
- Use sentence case: `Tags`, `Sources`, `Clear filters`, not spaced all-caps labels.
- Do not add decorative letter spacing to filenames, badges, or section titles.
- Filenames use the normal body style. Metadata labels use `.caption`/`.dim-label`; metadata values use body text.
- Counts use tabular/numeric styling where available and align to a common trailing edge.
- A hierarchy should require no more than three simultaneous weights: title, body, secondary.

---

## 4. Icon Policy

Use GNOME symbolic icons only. No emoji, Unicode approximations, multicolor icons, mixed icon families, or decorative icons beside headings.

An icon is justified when it represents a conventional compact action or status. If a reasonable user could interpret it two ways, pair it with a visible label. Tooltips and accessible names are still required for icon-only buttons.

| Location/action | Required treatment |
| --- | --- |
| Search | Native `SearchEntry` icon; no second search icon |
| Settings | `preferences-system-symbolic`, icon-only at header end |
| Sort | Visible `Sort` label plus native disclosure arrow; do not use a vertical-ellipsis icon |
| Thumbnail size | Slider only; remove decorative zoom-out/zoom-in icons |
| Clear filters | Visible `Clear filters (N)` text; neutral button, not suggested-action/pill |
| Source row | One folder icon at row start; `Offline` is visible text at row end |
| Media hover | Filename only; remove redundant image/video type icon |
| Placeholder | One centered image/video symbolic icon |
| Selection | Checkmark is allowed because it directly represents selected state |
| Viewer | Conventional previous, next, info, close, play/pause, mute, repeat, fullscreen icons |
| Empty state | One restrained folder symbolic icon, maximum 48px |

Toolbar icons use the native symbolic size, normally 16–20px. Viewer navigation icons may use 24px inside a minimum 44×44px target. Do not enlarge an error/empty-state icon to 96px.

---

## 5. Application Chrome

### Window

- Default size: 1200×800px. Minimum supported size: 960×600px.
- The sidebar remains 220px and the header remains scoped to the grid column.
- Do not draw a second border around the entire application content.

### Header

The visual order is:

```text
[Vesper]           [ Search media… ]     [Clear filters (N)] [size slider] [Sort ▾] [settings]
```

- Place `Vesper` at the start as the window title; the search entry is the centered title widget.
- Put Search inside an `adw::Clamp` title widget with a 280px tightening threshold and 360px maximum so it grows without crowding the end controls.
- `Clear filters (N)` appears only when filters/search are active. `N` is active tag count plus one when search is active.
- The size slider is 96px, has five detents, and has no flanking zoom icons or printed `XS–XL` labels. Its accessible value and tooltip expose the current size name.
- Sort is a labeled menu button. The active sort appears inside the popover, not as a changing header sentence.
- Settings remains the last control. No icon is placed merely to balance the header visually.
- The size slider and Sort control are adjacent but not forced into a `.linked` group; they are different actions.

### Sidebar

- Use flat navigation rows, not chip/pill rows.
- Section labels are `Tags` and `Sources` in sentence case.
- A tag row contains primary name, optional lineage below it only when disambiguation is needed, and a trailing count.
- Active tags use a 3px leading accent indicator plus a subtle accent-tinted row background. They do not become solid accent capsules.
- Hover uses a small native row background change. No border animation.
- Sources use the same flat-row language. Remove the card around the source list.
- The `Any`/`All` match choice is a plain labeled control row. It is not another floating capsule.

---

## 6. Grid and Media Cells

- Grid background uses `@view_bg_color`; outer padding and visible media-to-media gap are 12px. In GTK CSS this may be composed from a 4px `border-spacing` plus 4px margin on each neighboring cell so focus outlines are not clipped.
- Cells use 6px radius, no shadow, and no always-present border. A 3px internal/outer allowance prevents focus clipping.
- Default cells show only media and a video duration badge when known.
- Hover/focus reveals one filename line over the specified bottom gradient. Do not show a media-type icon; the content and duration badge already communicate type.
- The hover overlay fades in/out over 120ms. It does not move, scale, or change letter spacing.
- Focus uses the native/system accent at full contrast with a 2px outline and 2px offset.
- Selection uses a 2px accent border, top-left checkmark, and at most a 12% black tint. Never reduce the picture itself to 60% opacity.
- Duration uses a compact rectangular badge with 4px radius and normal/medium numeric weight, not a pill.

### Loading and failure

- Use a stable neutral placeholder with one small media-type icon.
- Do not use shimmer/skeleton animation in the grid. Many simultaneous shimmer animations are distracting, generic, and expensive.
- A visible cell actively waiting on a slow decode may show a native spinner after 400ms; do not show a spinner for cached loads that complete before that threshold.
- A thumbnail failure keeps the same placeholder and exposes the filename on hover/focus. It does not add a red error badge.

---

## 7. Selection Bar

- Attach the bar to the bottom edge of the grid area at full available width.
- Use an opaque `@headerbar_bg_color`/toolbar surface with a 1px top border and 12px horizontal padding. No floating capsule, large radius, or drop shadow.
- Layout: selected count at start; `Open location` and `Copy paths` as labeled neutral buttons; `Deselect all` at end.
- `Deselect all` is not destructive and must not use the destructive-action/red style.
- Action icons may accompany labels only when they are standard (`folder-open`, `edit-copy`). Never replace these labels with icons.

---

## 8. Viewer

- The 92% black scrim makes the media the only dominant object while retaining slight spatial context. Fullscreen is solid black.
- Opening/closing uses opacity only, at 120ms. Do not scale the entire viewer.
- Media has no card background, border, radius, or shadow. Preserve its native aspect ratio against the scrim.
- Close and Info live in one top-right OSD toolbar with 44px targets and 8px spacing. They are not separate oversized circles.
- Previous/next use full edge hit regions. Their OSD buttons appear on pointer proximity, keyboard focus, or recent navigation; icons remain fully opaque.
- The info panel is an opaque side panel with a leading border. It pushes the media area and is not a translucent floating card over the image.
- Video controls use one compact opaque OSD strip: play/pause, time, expanding seek, mute/volume, repeat, fullscreen. Use no decorative top-header gradient and no gradient behind an already-opaque control strip.
- Error states use a 48px maximum symbolic icon, a concise title, and optional one-line detail. Navigation and Close remain visually unchanged.

---

## 9. Motion

- Respect GTK's `gtk-enable-animations` setting. When animations are disabled, state changes are immediate.
- Standard durations: hover/focus 120ms; viewer fade 120ms; panel/action-bar reveal 160ms.
- Animate only opacity or a single panel reveal. Do not use `transition: all`.
- Do not animate grid layout, cell scale, shadows, border radius, or thousands of loading cells.
- Motion communicates state change; it is never continuous decoration.

---

## 10. Required Removal/Rework List

The visual refresh must remove or replace all of the following existing treatments:

- custom `#5a6b8c` accent definitions;
- hard-coded `#181818`/`#242424` application surfaces;
- pill tag rows and solid-accent active tags;
- the filled suggested-action filter pill;
- zoom-out/zoom-in icons around the size slider;
- vertical-ellipsis Sort icon;
- media-type icon in the filename hover overlay;
- 40% resting opacity on viewer controls;
- scale transform on viewer open/close;
- shimmer loading animation;
- grid-cell shadows and `transition: all`;
- 40% thumbnail darkening through picture opacity;
- floating shadowed selection capsule and destructive styling on `Deselect all`;
- source-list card and whole-row offline dimming;
- stacked gradients behind viewer/video controls;
- decorative all-caps/letter-spaced labels.

---

## 11. Recommended Implementation Order

Keep the refresh reviewable and avoid a single mixed rewrite:

1. **Foundations:** remove custom accent/hard-coded surfaces, define the spacing/radius rules, delete shimmer and broad transitions, and verify light/dark startup.
2. **Header:** move Search into the clamped title position, replace the filter pill and ellipsis Sort control, remove zoom icons, then verify minimum-width/large-text behavior.
3. **Sidebar:** convert section labels and tag/source rows, reserve active-indicator width, remove source card/whole-row dimming, then verify collisions and offline states.
4. **Grid:** remove shadows/type icon/picture-opacity selection, add the bounded tint and stable placeholder, then verify every cell state at all five sizes.
5. **Selection bar:** attach it to the grid edge, neutralize Deselect, and verify keyboard focus/order.
6. **Viewer:** remove scale/stacked gradients/low-opacity controls, create the combined top-right toolbar and opaque info panel, then verify bright/dark media and video errors.
7. **Motion and accessibility:** respect disabled animations, validate focus/high contrast/text scale, and ensure no affordance depends on hover alone.
8. **Acceptance:** capture reference screenshots for light/dark/high contrast and run Product responsiveness tests before declaring the refresh complete.

Each step must compile and preserve behavior. Do not combine visual cleanup with backend/schema changes.

---

## 12. Visual Acceptance Checklist

A UI change is complete only when all checks pass:

1. Light, dark, and high-contrast appearances use system colors with no unreadable hard-coded surface.
2. At 100%, 125%, and 200% text scaling, labels do not overlap and essential controls remain reachable.
3. Every icon-only control is conventional, has a tooltip/accessibility label, and has at least a 40×40px target (44×44px in the viewer).
4. Selection, focus, hover, offline, disabled, loading, and error states remain distinguishable without relying only on opacity or color.
5. The default grid contains no text or decoration beyond a known video duration.
6. Viewer controls are legible at rest and remain usable over both very bright and very dark media.
7. With animations disabled, no information or affordance disappears.
8. A screenshot review at default size shows one dominant layer—the media grid or open viewer—not several competing cards, pills, shadows, and gradients.
9. CSS contains no custom accent override, `transition: all`, continuous shimmer, or manual opacity on primary controls.
10. The Product performance budgets continue to pass after the visual change.
