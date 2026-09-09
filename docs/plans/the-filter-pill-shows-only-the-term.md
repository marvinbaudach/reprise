---
slug: the-filter-pill-shows-only-the-term
worktree: /home/marvin/Projects/reprise-the-filter-pill-shows-only-the-term
branch: feature/the-filter-pill-shows-only-the-term
phase: reviewed
created: 2026-09-09
base: origin/dev @ 41f1d3387e
---

# The filter pill shows only the term

The search chip in the filter bar currently renders as one flat button label:

    ⌕ “lorna” in track, artist and album  ×

The term — the only part the reader is looking for — carries the same colour and
weight as the scope clause around it, and the quotation marks are the only thing
marking where it starts. Design round 3, variant 3a of `Filter-Pill.dc.html`
resolves this: the magnifier marks the chip's origin, the term is the content,
and nothing else competes with it.

## What changes

**Search chip.** Magnifier icon, then the bare query, then a round remove
button. The scope clause (`in track, artist and album`, `in episode titles`, …)
and the typographic quotes are gone from the chip.

**Facet chip.** A chip that came from “+ Add filter” names its field as a short
muted prefix before the value — `Year 2022` — so the two kinds of chip stay
distinguishable without the search chip having to explain itself.

**The scope promise moves, it is not dropped.** FIL-1d's rule that a view may
never claim a field it does not search still holds; the search popover's own
caption (SEARCH-2c, `searches_scope`, `search_popover.rs`) keeps naming the
fields. Only the chip stops repeating it.

**Out of scope.** The place pill (FIL-1c) keeps its round outlined shape — it is
deliberately not a filter chip. “Clear all” is not mentioned by the brief and
keeps its current shape.

## Structure

Both chips become the same widget, replacing today's single `gtk4::Button`
whose label carried the `  ×`:

    Box  .reprise-filter-chip                     (horizontal, spacing 8)
    ├── Image  .reprise-filter-chip-icon          search chips only
    ├── Label  .reprise-filter-chip-field         facet chips only, muted prefix
    ├── Label  .reprise-filter-chip-value         the query or the facet value
    └── Button .reprise-filter-chip-remove        the ×

The container is not focusable and carries no accessible name. The remove button
takes over the accessible name the old chip button had — `Remove search: {query}`
for a search chip, `remove_filter_label(facet, value)` for a facet chip — so the
a11y contract of FIL-1a/FIL-1d survives instead of being moved into an
unlabelled focus stop.

## API

In `crates/reprise-gnome/src/ui/filter_bar_layout.rs`:

```rust
/// What a chip shows ahead of its value.
pub(in crate::ui) enum ChipLead<'a> {
    /// A chip that came from the search: the magnifier marks its origin.
    Search,
    /// A chip that came from "+ Add filter": its field, muted, as a prefix.
    Field(&'a str),
    /// A chip that is its own name, such as the "Hide AI music" toggle.
    Bare,
}

pub(in crate::ui) fn build_chip(
    lead: ChipLead<'_>,
    value: &str,
    accessible_remove_label: &str,
    on_remove: impl Fn() + 'static,
) -> gtk4::Box;

impl FilterBarLayout {
    /// The search chip: magnifier, the bare query, ×. Blank clears the slot.
    pub(in crate::ui) fn replace_search_chip(
        &self,
        query: &str,
        on_clear: impl Fn() + 'static,
    );
}
```

`replace_scoped_search` and `replace_search` both go away — every one of their
callers wants exactly this. That removes the `SearchScope` plumbing from the
chip path and collapses eleven call sites onto one function.

Correspondingly retired, after confirming no other caller:

- `filter_bar_strings::scoped_search_chip_label`
- `filter_bar_strings::chip_label`
- `reprise_view::strings::browse::search_chip_label_in` (msgid `⌕ “{query}” in {scope}`)
- `reprise_view::strings::browse::chip_label` (msgid `{facet}: {value}`)
- `preferences_search::settings_search_chip_label` (msgid for the settings chip)

`searches_scope`, `remove_search_label` and `remove_filter_label` all stay.

`browse_bar_chips::FilterChip` carries the two halves instead of one string:

```rust
pub(super) struct FilterChip {
    pub(super) facet: BrowseFacet,
    pub(super) field: String,
    pub(super) value: String,
    pub(super) accessible_remove_label: String,
}
```

`browse_bar::arm_smoke` logs `format!("{field} {value}")` where it logged
`chip.label`.

## Token mapping

The reference is authored in Nocturne's web tokens against one fixed dark
palette. This app is themed and its CSS is assembled from Rust token constants
over libadwaita named colours, so the *intent* is translated rather than the
literal values. `tokens.rs` records that the chip fill was already lowered from
0.22/0.32 to 0.14/0.18 because the label measured 4.17:1 and 3.37:1, below AA —
a hardcoded neutral fill would silently undo that finding. The chip is no longer
accent-tinted at all, which is what retires that constraint; carry the reasoning
into the new tokens' doc comment rather than deleting it.

| Reference | Implemented as | Note |
| --- | --- | --- |
| `--color-neutral-900` background | `alpha(@window_fg_color, 0.07)` | neutral, no longer accent-tinted |
| `--color-neutral-800` border | `alpha(@window_fg_color, 0.14)` | |
| `--color-accent` left edge 2px | `@accent_color` | unchanged intent |
| `--color-accent-400` icon | `@reprise_accent_text_color` | `contrast_5a_app_css_never_uses_unverified_accent_as_foreground` forbids raw `@accent_color` as a foreground; the derived value is the contrast-checked one. The left edge is a border, which that guard does not cover, so the two are close but not guaranteed identical. |
| `--color-neutral-100` value | `alpha(@window_fg_color, 0.95)` | `PRIMARY_TEXT_ALPHA` |
| `--color-neutral-500` field prefix | `alpha(@window_fg_color, 0.70)` | `SECONDARY_TEXT_ALPHA`, not 0.50: 12px text needs 4.5:1 |
| `--color-neutral-500` × resting | `alpha(@window_fg_color, 0.50)` | `HINT_TEXT_ALPHA`, a glyph with a hover state |
| `--color-neutral-800` × hover bg | `alpha(@window_fg_color, 0.12)` | |
| `--color-neutral-200` × hover fg | `alpha(@window_fg_color, 0.95)` | |
| `--radius-md` 8px | new `RADIUS_CHIP = "8px"` in `tokens.rs` | |
| `--color-neutral-700` dashed | `alpha(currentColor, 0.18)` dashed | the add-filter border already there |
| `--color-neutral-400` add-filter text | `alpha(@window_fg_color, 0.70)` | |
| 14px value | the inherited body size, `font-weight: 500` | absolute px ignores the user's font scale |
| 12px field prefix | the `caption` style class | ≈0.8125em, scales with the user's font |
| focus outline offset 2px | `FOCUS_RING_OFFSET` (1px) | the repository's own ring, used by every other button |

If `style::theme_tokens::ThemeTokens` already reaches `filter_bar_layout::css()`,
prefer its `pill_bg` / `pill_border` over the two surface alphas — they are the
same idea, already light/dark-selected. Do not restructure `app_css()` to make
that possible; the literals are acceptable otherwise.

## Geometry

- Chip container: `min-height: 36px`, radius 8, padding `0 6px 0 12px`, spacing 8.
- Remove button: 22×22, radius 11, no border, transparent background, `padding: 0`.
- `+ Add filter`: `min-height: 36px`, radius 8, 1px dashed, muted text. Drop its
  `pill` style class in `browse_bar.rs` — libadwaita's `pill` forces a 9999px
  radius that would defeat the 8px corner. The dashed outline goes on the
  `MenuButton`'s `button` child, not on the class's own node: the outer node
  paints nothing, and a border authored there renders as no border at all
  (measured 2026-09-10 from the captured bar, in both themes). Its alpha is
  0.30 rather than the 0.18 the old rule carried — at 0.18 on the child node
  the line was present but too faint to read as a border.
- `FILTER_BAR_MIN_HEIGHT` stays 34. It is a floor and 36 clears it; raising it
  would move list geometry that has nothing to do with this change.
- `CHIP_MIN_HIT_PX` (20, the FIL-1a click-target floor) stops sizing the whole
  chip and becomes the floor the 22px remove button must clear. Assert that
  relation rather than deleting the constant.

## Icon

`gtk4::Image::from_icon_name("system-search-symbolic")` at pixel size 13.
`ui/icons.rs` already treats that name as available; verify against the guard
there before relying on it.

## Work

1. `filter_bar_layout.rs` — `ChipLead`, `build_chip`, `replace_search_chip`,
   the retired functions, the new CSS section.
2. `style/tokens.rs` — `RADIUS_CHIP` and the chip surface/text alphas; retire
   `CHIP_BG_ALPHA` / `CHIP_BG_HOVER_ALPHA` and carry their AA reasoning over.
3. `browse/browse_bar_chips.rs` + `browse/browse_bar.rs` — split `FilterChip`,
   build facet chips with `ChipLead::Field`, the "Hide AI music" chip with
   `ChipLead::Bare`, drop the `pill` class from `+ Add filter`, fix `arm_smoke`.
4. Every `replace_scoped_search` / `replace_search` caller: `shortcuts.rs` (×2),
   `radio/`, `releases/`, `concerts/`, `podcasts/`, `browse/browse_bar.rs`,
   `library_doctor/review_filter_bar.rs`, `preferences/preferences_search.rs`.
   `browse/filter_bar.rs` is dead code behind `allow(dead_code)` but must keep
   compiling — port it, do not rewrite it.
5. `crates/reprise-view/src/strings/browse.rs` — retire the two dead messages.
6. Tests: every assertion listed under *Verification* below.
7. `docs/ux-rules.md` FIL-1a and FIL-1d, plus the chip-wording quotes near
   line 6601. Handled outside the implementation pass.

## Verification

- `cargo check -p reprise-gnome --all-targets` and `cargo clippy` clean.
- `scripts/tests/gettext-catalogs.sh` — no new msgid is introduced, so `msgcmp`
  must stay green; retired msgids leave harmless unused entries behind.
- The display suite is what actually measures this widget. Its tests are
  `#[ignore = "requires a display; run via xvfb-run"]`; a green plain
  `cargo test` proves nothing here. Run them under `xvfb-run`.
- Assertions that change: `filter_bar_layout.rs` (the search-slot test now finds
  a `gtk4::Box`, not a `Button`), `filter_bar_strings.rs` (the scoped-label
  tests go with the function), `window/section_search/tests.rs`,
  `window/search_popover_tests.rs`, `browse/browse_bar_tests.rs`,
  `browse/filter_bar_tests.rs`, `library_doctor/review_search_tests.rs`.
