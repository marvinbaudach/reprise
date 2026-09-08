---
slug: the-lightmode-gets-its-own-edges
worktree: /home/marvin/Projects/reprise-the-lightmode-gets-its-own-edges
branch: feature/the-lightmode-gets-its-own-edges
phase: planned
codex_session:
created: 2026-09-08
---
# The lightmode gets its own edges

## Why

Every hairline, tint and elevation rung in the GTK frontend is authored as a
literal `rgba(255, 255, 255, …)` or `alpha(white, …)`. On the dark palettes
that is correct — white at 4.5 % over `#1b1e22` is exactly the separator the
design asks for. On the light palettes the same literal paints white over
near-white and disappears. With no edges left, the mint accent is the only
structure the eye can find, which is why light reads flat and shouty.

The repository already knows the better pattern in one place: the player bar's
top edge is `alpha(@window_fg_color, 0.07)` — a foreground tint, so it flips
with the appearance on its own. This plan generalises that, without touching
the dark result.

## The hard rule

**The dark appearance must not change by a single pixel.** Every change lands
one of two ways:

1. as a new `@define-color` token emitted by `theme_css`, whose **dark branch
   reproduces today's literal character for character**, or
2. as an explicitly `is_dark`-guarded value inside `theme_css`.

`crates/reprise-gnome/src/ui/style/mod.rs` loads two providers: `app_css()`
(appearance-independent, built once) and `theme::theme_css(theme, is_dark,
source)` (rebuilt on every appearance/theme/accent change, `mod.rs:227`).
**`app_css()` therefore may not learn about `is_dark`.** Any light-mode
difference has to travel through a token defined in `theme_css` — this is not a
style preference, it is the only place the appearance is known.

Consequence for values that must *vanish* in one appearance: emit them as a
token whose other branch is fully transparent, e.g.
`@define-color reprise_play_glow_near alpha(@reprise_player_accent, 0);`. A
shadow layer painted with a zero-alpha colour renders nothing, so the rule text
stays identical in both appearances.

## What is already correct — do not "fix" it

The task list this plan comes from was written against an older reading of the
code. These four items are already implemented and **must be left alone**;
changing them would move dark pixels for no light-mode gain:

| Claimed defect | Actual state | Verdict |
| --- | --- | --- |
| "Running row uses a full accent flood" | `track_list_row_interaction.rs:24-27` already paints `alpha(@accent_color, 0.09)` + `inset 2px 0 0 @accent_color` + `.now-playing-title { color: @reprise_accent_text_color; font-weight: bold; }` | No flood exists. Only the light tint is nudged (step 6); the 2 px bar and the bold title stay. |
| "Rating stars need an accent/dim split" | `tag_edit/tag_editor_style.rs:210-213` already has `.star-filled { color: @reprise_accent_text_color; }` and `.star-outline { color: alpha(@window_fg_color, 0.35); }` | Done. `0.35` vs. the requested `0.32` is below perception and would move dark. **No change.** |
| "Now Playing summary sits in an overlay pill" | `now_playing/surface_css.rs:43` `.reprise-now-playing-footer` is already a footer caption inside the panel | Done. Only the *library* pill is an overlay (step 8). |
| "Shuffle/repeat active needs accent text, no slab" | `style/buttons.rs:207-217` already uses `alpha(@accent_bg_color, 0.18)` + `@reprise_accent_text_color` + a dot | Only the light fill alpha is lowered (step 7). |

Two further deviations from the source task, both deliberate:

- **The play button stays 44 px** (`player_bar_layout.rs:34
  PLAY_BUTTON_SIZE = 44`). The task asked for a 40 px circle in light; a size
  is not expressible as a colour token, it would need `app_css()` to branch on
  appearance, and "row height / density" is out of scope. Only the *glow* goes.
- **`accent_color` is not re-derived.** See step 3.

## Scope

`crates/reprise-gnome/src/ui/` only. No Kotlin, no Android, no core crate, no
schema, no strings/`.po` files, no new dependencies.

---

## Step 1 — the token block in `theme_css`

File: `crates/reprise-gnome/src/ui/style/theme.rs`, function `theme_css`.

Add the light values as constants next to their dark counterparts in
`crates/reprise-gnome/src/ui/style/tokens.rs` (same doc-comment style as the
neighbours; each one says which appearance it serves and why the dark twin
cannot be reused).

Emit the following block from `theme_css`. The **dark column is the contract**:
each string must come out byte-identical to the literal it replaces.

| Token | Dark value (emitted verbatim) | Light value |
| --- | --- | --- |
| `reprise_hairline` | `rgba(255, 255, 255, 0.06)` | `rgba(0, 0, 6, 0.09)` |
| `reprise_hairline_strong` | `rgba(255, 255, 255, 0.07)` | `rgba(0, 0, 6, 0.11)` |
| `reprise_rule` | `rgba(255, 255, 255, 0.045)` | `rgba(0, 0, 6, 0.055)` |
| `reprise_pill_border` | `rgba(255, 255, 255, 0.10)` | `rgba(0, 0, 6, 0.14)` |
| `reprise_pill_bg` | `@sidebar_bg_color` | `@card_bg_color` |
| `reprise_hover_bg` | `alpha(@accent_bg_color, {HOVER_BG_ALPHA})` | `alpha(@window_fg_color, 0.045)` |
| `reprise_now_playing_tint` | `alpha(@accent_color, 0.09)` | `alpha(@accent_bg_color, 0.12)` |
| `reprise_now_playing_glow` | `alpha(@reprise_player_accent, {NOW_PLAYING_GLOW_ALPHA})` | `alpha(@reprise_player_accent, 0.05)` |
| `reprise_cover_edge` | `alpha(@sidebar_fg_color, 0.12)` | `alpha(@window_fg_color, 0.10)` |
| `reprise_cover_shadow` | `alpha(#000000, 0)` | `alpha(#000000, 0.16)` |
| `reprise_tab_active_bg` | `alpha(@sidebar_fg_color, {NOW_PLAYING_PILL_ACTIVE_ALPHA})` | `@view_bg_color` |
| `reprise_tab_active_shadow` | `alpha(#000000, 0)` | `alpha(#000000, 0.14)` |
| `reprise_toggle_checked_fill` | `alpha(@accent_bg_color, {BTN_CHECKED_FILL_ALPHA})` | `alpha(@accent_bg_color, 0.14)` |
| `reprise_play_glow_near` | `alpha(@reprise_player_accent, 0.60)` | `alpha(@reprise_player_accent, 0)` |
| `reprise_play_glow_far` | `alpha(@reprise_player_accent, 0.35)` | `alpha(@reprise_player_accent, 0)` |
| `reprise_play_glow_near_hover` | `alpha(@reprise_player_accent, 0.75)` | `alpha(@reprise_player_accent, 0)` |
| `reprise_play_glow_far_hover` | `alpha(@reprise_player_accent, 0.48)` | `alpha(@reprise_player_accent, 0)` |
| `reprise_play_ring` | `alpha(@window_fg_color, 0)` | `alpha(@window_fg_color, 0.12)` |
| `reprise_play_drop` | `alpha(#000000, 0.36)` | `alpha(#000000, 0.18)` |
| `reprise_play_drop_hover` | `alpha(#000000, 0.34)` | `alpha(#000000, 0.22)` |

`{…}` names are the existing `tokens.rs` constants, interpolated — do not
inline their current values, so a later retune still flows through.

Ordering matters: the block must come **after** `accent_css` and after
`@define-color reprise_player_accent`, because several entries reference
`@accent_bg_color`, `@accent_color` and `@reprise_player_accent`. Referencing a
name libadwaita owns (the `AccentSource::System` case, where `accent_css` is
empty) is already done today by `reprise_player_accent` and works.

Note why the light tint is built from `@accent_bg_color` while the dark one
keeps `@accent_color`: under `AccentSource::System` those two names are
*different* colours (libadwaita derives its own contrast-safe `accent_color`),
so dark has to keep the exact name it uses today. In light, `accent_color`
becomes the darkened teal (step 3), and `Palette::critical_accent_surface`
models every accent tint as being made from the **raw** accent — building the
light tint from `@accent_color` would paint a surface the contrast proof never
measured. `@accent_bg_color` is the raw accent in both appearances, so the
model and the paint agree.

**Verify the grammar with one token before writing the rest.** The repo has
precedent for a bare alias (`@define-color reprise_player_accent @accent_color;`)
but not for `rgba(…)` or `alpha(@name, x)` on the right-hand side of a
`@define-color`. Add `reprise_hairline` first, run the crate's style tests, and
only then fill in the table.

### Test (step 1)

In `theme.rs`'s test module, one test per appearance:

- `dark_hairline_tokens_reproduce_the_literals_they_replaced`: for every
  `Theme::all()` × both `AccentSource`, assert `theme_css(theme, true, source)`
  contains, verbatim:
  `@define-color reprise_hairline rgba(255, 255, 255, 0.06);`,
  `@define-color reprise_hairline_strong rgba(255, 255, 255, 0.07);`,
  `@define-color reprise_rule rgba(255, 255, 255, 0.045);`,
  `@define-color reprise_pill_border rgba(255, 255, 255, 0.10);`.
  Write the expected strings out as literals — deriving them from the same
  constants the production code uses would let the test vouch for itself.
- `light_hairlines_are_dark_on_light`: same sweep with `is_dark = false`,
  asserting the light strings are present and that no
  `rgba(255, 255, 255,` appears in any hairline definition.

---

## Step 2 — replace the literals

Five call sites. Nothing but the colour changes; keep width, style and
selectors exactly as they are.

| File | Rule | Was | Becomes |
| --- | --- | --- | --- |
| `ui/window/library_chrome_css.rs` | `.reprise-library-split .reprise-library-sidebar` `border-right` | `rgba(255, 255, 255, 0.06)` | `@reprise_hairline` |
| `ui/window/library_chrome_css.rs` | `.reprise-library-header` `border-bottom` | `rgba(255, 255, 255, 0.06)` | `@reprise_hairline` |
| `ui/track_list/track_list_header_style.rs` | `.reprise-track-list > header` `border-bottom` | `rgba(255, 255, 255, 0.07)` | `@reprise_hairline_strong` |
| `ui/track_list/track_list_header_style.rs` | `.reprise-track-list > listview > row > cell` `border-bottom` | `rgba(255, 255, 255, 0.045)` | `@reprise_rule` |
| `ui/now_playing/surface_css.rs` | `.reprise-now-playing-stage` `border-left` | `rgba(255, 255, 255, 0.06)` | `@reprise_hairline` |

**The module doc of `track_list_header_style.rs` currently argues for the
literals** ("The `rgba(white)` literals are deliberate — these are fixed
hairlines on the dark surface, not theme-tinted borders, so they don't route
through a palette `@`-color"). That reasoning is what broke light. Replace that
paragraph: the hairlines are still fixed, not theme-tinted, but they are now
*appearance*-dependent, and `theme_css` is where the appearance is known.

`header_style_is_subtle_and_scoped_away_from_song_cells` keeps passing as
written; extend it to assert the two tokens are referenced and that no
`rgba(255, 255, 255` literal survives in this file's output.

---

## Step 3 — the accent text role in light

File: `crates/reprise-gnome/src/ui/style/accent.rs` + `theme.rs`.

Today `css_overrides` emits `accent_color` and `accent_bg_color` both as
`APP_ACCENT`. On the light palettes the brand teal `#4fdbd4` cannot carry text
or glyphs.

**Do not add a second derivation.** `theme_css` already computes
`accent_text` — `accent_text_color(accent, p.critical_accent_surface(is_dark,
accent), is_dark)` — which is derived against the *critical* surface (the
palette's worst case including accent-tinted rungs), and is therefore strictly
stronger than the 4.5:1-against-`@view_bg_color` the task asked for. Reuse it.

Change:

- `accent_bg_color` stays `APP_ACCENT` in both appearances (fills keep the
  brand colour).
- `accent_fg_color` unchanged.
- `accent_color` becomes `APP_ACCENT` when `is_dark`, and the already-computed
  `accent_text` value when light.

Mechanically: give `css_overrides` a second parameter, e.g.
`css_overrides(source: AccentSource, accent_color: &str)`, and have `theme_css`
pass `if is_dark { APP_ACCENT } else { &accent_text }`. `AccentSource::System`
keeps returning an empty string — libadwaita owns all three roles there, and
libadwaita derives its own contrast-safe `accent_color`.

Why not the literal formula from the task
(`ensure_contrast_by_lightness(APP_ACCENT, view_bg, is_dark, 4.5)`): in the
dark appearance that call does **not** return `APP_ACCENT`. `accent.rs`'s own
test comment states it — *"The brand teal does not clear the heaviest accent
tint on its own — that is the whole reason the role is derived rather than
aliased"* — and `contrast_5a_the_derived_accent_stays_the_accent_lifted`
asserts the derived value is lifted away from the accent. Since
`@define-color reprise_player_accent @accent_color;` feeds the play button, the
equaliser and the waveform, an unguarded change there would repaint the dark
appearance. Hence the `is_dark` guard.

### Tests (step 3)

- `dark_keeps_the_brand_accent_for_every_role`: for every `Theme::all()`,
  `theme_css(theme, true, AccentSource::App)` contains
  `@define-color accent_color {APP_ACCENT};` **and**
  `@define-color accent_bg_color {APP_ACCENT};`. This is the regression fence
  for the whole step.
- `light_accent_text_clears_aa_on_the_view_background`: for every theme, parse
  the emitted light `accent_color` and assert `contrast_ratio` against
  `light_palette().view_bg` is `>= 4.5`, and that it differs from `APP_ACCENT`.
- `light_accent_text_clears_aa_on_the_running_row`: the pair that is actually
  painted, which no existing test covers. `.now-playing-title` uses
  `@reprise_accent_text_color` on a row filled with `reprise_now_playing_tint`.
  Assert
  `contrast_ratio(accent_text, composite(effective_accent_rgb(source), view_bg, 0.12)) >= 4.5`
  for every theme and both sources, light appearance.
  **Run this check before implementing step 6.** If it fails, the light tint
  alpha comes down until it clears — do not weaken the accent-text role, and do
  not switch the tint back to `@accent_color`.
- `system_accent_still_defines_no_adwaita_roles`: the existing
  `system_accent_css_leaves_adwaita_roles_undefined_and_keeps_player_alias`
  must keep passing unmodified in both appearances.

The existing `contrast_5a_*` suite in `accent.rs` and `tokens.rs` must stay
green untouched.

---

## Step 4 — list hover darkens in light

File: `crates/reprise-gnome/src/ui/style/interactions.rs`.

`.reprise-hover:hover { background-color: alpha(@accent_bg_color, {HOVER_BG_ALPHA}); }`
becomes `background-color: @reprise_hover_bg;`.

**Only that one rule.** `.reprise-panel-toggle:checked` and
`.reprise-panel-toggle:checked:hover` also read `HOVER_BG_ALPHA` /
`HOVER_BG_ALPHA_STRONG`, but those are accent *state* fills, not hover
feedback — leave them on the accent.

`tokens.rs`'s `contrast_3_hover_tints_leave_text_above_aa` models hover as an
accent tint. That stays true in dark. Extend the test's doc comment (or add a
light arm) to record that the light appearance now hovers with a foreground
tint at 0.045, which is strictly gentler than the accent tint it replaces and
so cannot lower any ratio the test guards.

---

## Step 5 — the play button loses its glow in light

File: `crates/reprise-gnome/src/ui/player_bar/player_bar_layout.rs`.

Rewrite the three `box-shadow` lists so the accent glow layers and the drop
shadow read from tokens, and add one ring layer. Keep the inset highlight and
inset sink literals as they are — they are lighting, not colour, and their
alphas already work on both grounds.

- `.player-bar-play`:
  `inset 0 2px 1px alpha(#ffffff, 0.34), inset 0 -4px 3px alpha(#000000, 0.30), inset 0 0 0 1px @reprise_play_ring, 0 6px 12px @reprise_play_drop, 0 0 12px @reprise_play_glow_near, 0 0 26px 6px @reprise_play_glow_far`
- `.player-bar-play:hover`: same shape with
  `@reprise_play_drop_hover`, `@reprise_play_glow_near_hover`,
  `@reprise_play_glow_far_hover`, keeping the existing inset literals
  (`0.42` / `0.26`) and the `0 7px 14px` geometry.
- `.player-bar-play:active`: keep its colours as they are — the outer layers
  are a press ring in the playback accent, and on a light ground a 4 px accent
  ring is still the right press answer. **But it must gain the same
  `inset 0 0 0 1px @reprise_play_ring` layer** as the other two states.

All three lists must end up with the **same number of `box-shadow` layers**.
`.player-bar-play` carries `transition: box-shadow {TRANSITION}`, and GTK
interpolates shadow lists layer by layer; a base state with six layers
transitioning into an `:active` state with five does not animate the way the
matched pair does. The ring is free in dark (`alpha(@window_fg_color, 0)`
paints nothing), so adding it everywhere costs nothing and keeps the lists
aligned. Count the layers in all three rules before committing.

Dark identity: `@reprise_play_ring` is `alpha(@window_fg_color, 0)` in dark, so
the added inset layer paints nothing; every other token reproduces its former
literal.

**Leave the `PLAY-16` comment block in place** and append one sentence: the
1.69:1 white-on-accent ratio it records is the dark measurement, and in light
`@reprise_player_accent` now follows the derived `accent_color`, which raises
it. If `scripts/measure-contrast.py` or `docs/ux-rules.md` pins the 1.69 figure
to an appearance-independent claim, update the wording there too — do not
change the exemption itself.

---

## Step 6 — the Now Playing panel

File: `crates/reprise-gnome/src/ui/now_playing/surface_css.rs`.

- `.reprise-now-playing-glow`: the gradient's inner stop becomes
  `@reprise_now_playing_glow` (0.15 dark, 0.05 light). The outer stop
  `alpha(@sidebar_bg_color, 0)` is unchanged.
- `.reprise-now-playing-cover`:
  `box-shadow: inset 0 0 0 1px @reprise_cover_edge, 0 2px 6px @reprise_cover_shadow;`
- `.reprise-now-playing-tabs button:checked`:
  `background-color: @reprise_tab_active_bg; box-shadow: 0 1px 2px @reprise_tab_active_shadow;`
  Colour and weight (`@reprise_primary_fg_color`, `font-weight: 700`) unchanged.

File: `crates/reprise-gnome/src/ui/track_list/track_list_row_interaction.rs`.

- `.reprise-track-cell.now-playing { background-color: @reprise_now_playing_tint; }`
  The `.now-playing-leading` bar stays at `inset 2px 0 0 @accent_color` and
  `.now-playing-title` keeps `font-weight: bold`. Widening the bar to 3 px is
  not expressible as a token and would move dark pixels.

---

## Step 7 — the shuffle/repeat fill in light

File: `crates/reprise-gnome/src/ui/style/buttons.rs`.

`.reprise-btn-toggle:checked` `background-color` becomes
`@reprise_toggle_checked_fill` (0.18 dark, 0.14 light). The `:checked:hover`
and `:checked:active` fills and the radial dot stay exactly as they are.

---

## Step 8 — the library summary pill becomes readable

File: `crates/reprise-gnome/src/ui/track_list/track_content.rs`.

The pill is a real floating overlay (`gtk4::Overlay::add_overlay`, bottom-right,
16 px inset), and its border is `rgba(255, 255, 255, 0.10)` — invisible in
light, and it floats over rows with a `@sidebar_bg_color` fill that is nearly
the same colour as the table underneath it.

`.reprise-list-status-bar`:

- `border: 1px solid @reprise_pill_border;`
- `background-color: @reprise_pill_bg;` — dark keeps `@sidebar_bg_color`
  exactly as today; light lifts to `@card_bg_color` (`#ffffff` in all three
  light palettes) so the pill separates from the table it floats over.
- `box-shadow: 0 1px 3px @reprise_cover_shadow;` — transparent in dark, so the
  dark pill is untouched.

**The pill is not moved.** Relocating it into the library toolbar is a
structural change that cannot be branched by appearance without reparenting a
widget on every `StyleManager::dark` notification, and it would diverge the two
appearances' widget trees. If the pill should leave the overlay, that is a
separate change for both appearances.

---

## Step 9 — the sidebar toggle stops looking checked

Files: `crates/reprise-gnome/src/ui/style/buttons.rs` (or wherever the rule
lands most naturally) and `crates/reprise-gnome/src/ui/shortcuts.rs` for the
class name constant `SIDEBAR_TOGGLE_CSS_CLASS = "reprise-sidebar-toggle"`.

The sidebar toggle is a `GtkToggleButton` carrying `.reprise-btn-toggle`
(`window_navigation.rs:171`), so it inherits the accent fill and the state dot
from `.reprise-btn-toggle:checked`. Showing the sidebar is not a mode the user
is *in*, it is where the sidebar is; the accent slab reads as an unrelated
filter being active.

Add, after the `.reprise-btn-toggle:checked` rules so it wins on order:

```
.reprise-sidebar-toggle:checked {
  background-color: transparent;
  background-image: none;
  color: inherit;
}
.reprise-sidebar-toggle:checked:hover {
  background-color: alpha(currentColor, {BTN_HOVER_ALPHA});
}
.reprise-sidebar-toggle:checked:active {
  background-color: alpha(currentColor, {BTN_PRESS_ALPHA});
}
```

**This one applies to both appearances — that is confirmed and intended.** The
window-control *order* (close/minimize placement) is explicitly **not** part of
this change.

Test: a unit test asserting the generated CSS contains a
`.reprise-sidebar-toggle:checked` rule that sets `background-image: none`, and
that `shortcuts::SIDEBAR_TOGGLE_CSS_CLASS` is the selector used (so a rename
cannot silently orphan the rule).

---

## Out of scope — recorded, not built

- **The dialog elevation ladder.** `style/interactions.rs:50-58` paints
  `floating-sheet > sheet` outline, its headerbar tint and its `.boxed-list`
  with `alpha(white, …)`. In light these are white-on-white and the dialog loses
  its border and card separation — the same root cause as the hairlines. It is
  left out because `Palette::critical_accent_surface` (`theme.rs`) *models*
  those rungs as white composites when it picks the worst-case surface for the
  accent-text derivation; flipping them to a dark tint in light changes which
  surface is critical and therefore re-opens the whole `contrast_5a` proof.
  Worth its own change, with the model updated in the same commit.
- Spectrum seek in the player bar, row height/density, the accent-source
  default (`AccentSource::DEFAULT` stays `App`), and anything visible only in
  dark.

---

## Verification scope

This change touches `crates/reprise-gnome/src/ui/` only — Rust, one crate, no
Kotlin, no Android, no schema, no generated bindings.

Run:

```
cargo fmt
cargo clippy -p reprise-gnome --all-targets -- -D warnings
cargo test -p reprise-gnome --lib
scripts/check-gnome-idioms.sh
```

Do **not** run: `cargo test --workspace`, `cargo build --workspace`,
`cargo audit`, `gradlew`, `uniffi-bindgen`, the Android suite, or any
repo-wide gate script. If `AGENTS.md` or a gate document tells you to run the
full gate before committing, that instruction does not apply to this run —
this exception is deliberate and stated here. A previous change of this shape
burned thirty-one minutes on a workspace cargo build and an Android FFI release
build before being killed, and committed nothing.

The display-suite tests (`#[ignore = "requires a display"]`) are **not**
required for acceptance and must not be run here.

## Acceptance

1. `cargo test -p reprise-gnome --lib` green, including the new tests named in
   steps 1, 3 and 9.
2. **Dark identity, proved textually:** the new step-1 dark test asserts every
   replaced literal comes back out of `theme_css(theme, true, source)` for all
   three themes and both accent sources. Since `app_css()` never learns about
   the appearance, and every edited rule now resolves through those tokens,
   text identity is pixel identity here. State in the commit body which literal
   maps to which token.
3. No production rule outside `theme.rs` still paints a hairline with a white
   literal. Check with

   ```
   grep -rn 'rgba(255, 255, 255' crates/reprise-gnome/src/ui/ \
     | grep -v '/style/theme.rs:'
   ```

   which must come back **empty**. `theme.rs` is excluded because both its dark
   token branch and the step-1 test's expected strings legitimately contain the
   literals — that is the point of the test. `style/interactions.rs` keeps its
   `alpha(white, …)` elevation rungs (different spelling, out of scope above)
   and is not matched by this grep.
4. `scripts/check-gnome-idioms.sh` green.
5. `cargo clippy -p reprise-gnome --all-targets -- -D warnings` clean.

## Ownership

Start here; the list is a starting point, not a fence. If a contract needs a
file that is not named, take it and say so in the report.

- `crates/reprise-gnome/src/ui/style/theme.rs` — token block, dark/light branch
- `crates/reprise-gnome/src/ui/style/tokens.rs` — new light constants
- `crates/reprise-gnome/src/ui/style/accent.rs` — `css_overrides` signature
- `crates/reprise-gnome/src/ui/style/interactions.rs` — hover rule
- `crates/reprise-gnome/src/ui/style/buttons.rs` — toggle fill, sidebar toggle
- `crates/reprise-gnome/src/ui/window/library_chrome_css.rs`
- `crates/reprise-gnome/src/ui/track_list/track_list_header_style.rs`
- `crates/reprise-gnome/src/ui/track_list/track_list_row_interaction.rs`
- `crates/reprise-gnome/src/ui/track_list/track_content.rs`
- `crates/reprise-gnome/src/ui/now_playing/surface_css.rs`
- `crates/reprise-gnome/src/ui/player_bar/player_bar_layout.rs`
- `crates/reprise-gnome/src/ui/shortcuts.rs` — read the class constant
</content>
