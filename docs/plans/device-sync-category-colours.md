---
slug: device-sync-category-colours
worktree:
branch:
phase: planned
codex_session:
created: 2026-08-08
---
# The categories get their own colours, and a legend to read them by

Draft **2c** of `Reprise Sync UX.dc.html` colours the "Up next" card by source —
Music teal, YouTube green, Podcasts blue — in the storage bar, in the row icons,
and in a legend under the bar. PR #336 shipped the card's structure and none of
its colour: `CategoryStorageBar` paints every segment in `widget.color()` at
four opacities (0.82 / 0.62 / 0.42 / 0.22), the row icons mirror those same
opacities, and there is no legend at all. The categories are distinguishable
only by brightness, which is exactly what 2c set out to fix.

The cause was the plan, not the implementation: #336's plan specified row shape,
rule sentence, result column and summary line, and said nothing about hue, so
the existing monochrome scheme was carried forward. It is internally consistent
and it is not the design.

## Where the colours live, and why not `palette.toml`

`data/brand/palette.toml` is the brand-asset pipeline's source — logos, icons,
favicons, built by `scripts/build-brand-assets.sh`, which reads only
`reprise_teal` and `reprise_coral`. The application never reads it. App colour
lives in `crates/reprise-gnome/src/ui/style/`: libadwaita named roles, the brand
accent literal in `accent.rs`, per-theme surfaces in `theme.rs`, and alphas in
`tokens.rs`. Category colours belong there.

**They are not per-theme.** `Palette` carries nine surface and text colours for
each of three themes in light and dark; adding three category hues there would
mean eighteen new literals and would let "Music" change colour when the user
changes theme. A category's colour is its identity — it stays put. Define one
tone per category with a light-mode variant, the way `accent.rs` already holds
`APP_ACCENT` plus its foreground.

**They do not follow the accent source either.** `AccentSource::System` lets the
user hand the accent to the desktop. If Music simply *were* the accent, its
colour would move with that setting and could collide with YouTube's green.
Music reuses the `APP_ACCENT` literal as a fixed value, not the resolved accent.

## Task 1 — Three category tones

New `crates/reprise-gnome/src/ui/style/category_colors.rs`, exported from
`style/mod.rs`:

- Music: reuse the existing `APP_ACCENT` literal (`#4FDBD4`) rather than
  restating it, plus a light-mode variant dark enough to read on a light
  surface.
- YouTube: `#3FCB8E` dark / `#1B7A50` light.
- Podcasts: `#7C9BEE` dark / `#3355B5` light.

Expose `pub(in crate::ui) fn category_color(kind: SyncTargetKind, is_dark: bool)
-> &'static str`. `SyncTargetKind` is the type the panel and the bar already
key on, so nothing needs a second enum.

Check contrast rather than trusting the hexes: each tone must stay legible as an
icon and as a bar segment on that mode's `card_bg` across all three themes.
`accent.rs` records the accent's measured ratio in a doc comment — do the same
here, and adjust a value if it does not hold. Say which you changed and why.

## Task 2 — The bar shows hue, not brightness

`device_sync_category_bar.rs`

- Music, YouTube and Podcasts segments take their category colour at full
  opacity instead of `widget.color()` at 0.82 / 0.62 / 0.42.
- "Other" stays neutral — it is not a category, it is everything else. Keep it
  on `widget.color()` at its current low alpha.
- The hatched "incoming this sync" segment keeps its hatching, which is what
  makes it read as "about to change" rather than "another kind of content".
  Hatch it in the colour of whatever it is incoming *for* if that is cheap to
  know; otherwise keep it on the foreground and say so.
- Keep the track added in #336 (`rounded_track`, 8% foreground) — it is what
  makes a nearly empty device read as a bar with room in it.
- The widget must re-read its colours when the theme flips light/dark. It
  currently reads `widget.color()` inside the draw function, which is
  automatically current; a `const` looked up once at construction would not be.
  Resolve `is_dark` inside `set_draw_func`, not before it.

## Task 3 — The row icon matches its segment

`device_sync_content_panel.rs:380-393`

The icon currently encodes its category by `set_opacity` — a comment there says
the icon and its segment should stay "one visual key", which is right, and the
key just changed. Replace the opacity trick with the category colour.

GTK symbolic icons recolour through CSS, so give each row's icon a CSS class and
emit the three rules from the app's stylesheet the way the rest of
`ui/style/` does; do not fight `GtkImage` with inline attributes. Remove the
opacity ladder entirely rather than leaving it under the colour.

## Task 4 — The legend

Under the bar, before the category rows: one entry per *visible* category, each
a small colour swatch plus the category name and its size on the device —
`● Music 1.1 GiB   ● YouTube 693 MiB   ● Podcasts 217 MiB`.

- Honour `MTP-46`: a switched-off source has no row, so it has no legend entry
  either. The panel already computes that visibility once; read it, do not
  recompute.
- The sizes are the same numbers the rows show. Take them from the same place —
  a legend that can disagree with the row above it is worse than no legend.
- Strings through `device_sync_strings` as usual; `de` and `es` are
  required-complete, and a msgstr copied from the English source passes the gate
  while translating nothing.

## Out of scope

- `palette.toml` and the brand-asset pipeline.
- The history card, the hero, the picker.
- Any change to what the numbers mean.

## Verification

- `cargo test -p reprise-core -p reprise-gnome -p reprise-platform-linux`.
- `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`.
- `scripts/tests/gettext-catalogs.sh`.
- Display-backed tests need `GDK_BACKEND=x11` under Xvfb with `WAYLAND_DISPLAY`
  unset, one process at a time.
- `scripts/check-architecture.sh` is already red on `origin/dev`
  (`crates/reprise-core/src/library/tag_edit_write.rs`, 824 lines). Confirm that
  is still the only violation rather than assuming it.
- A unit test that the three categories resolve to three *different* colours in
  both modes — the failure this whole change is about is three things looking
  the same, and that is cheap to assert.
