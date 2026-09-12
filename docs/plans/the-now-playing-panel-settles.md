---
slug: the-now-playing-panel-settles
worktree: /home/marvin/Projects/reprise-the-now-playing-panel-settles
branch: feature/the-now-playing-panel-settles
phase: shipped
codex_session:
created: 2026-09-11
---
# The now-playing panel settles

Source: `docs/superpowers/specs/2026-09-11-now-playing-panel-refinement-design.md`
(Claude Design "Cover Varianten", variant 4b, scaled 300/430). GNOME panel only;
Android untouched. Base: `origin/dev` at `2eb70d5152` — the clouds (#925) and
their longer drift (#927) are already in, so this plan changes only stacking,
fades, head geometry, the title block, the segment control and the transition
to the list.

## As-is map (origin/dev, `crates/reprise-gnome/src/ui/now_playing/`)

```
head_group (Overlay)                                   now_playing.rs:185-190
  child   glow            .reprise-now-playing-glow    NPP-3 radial accent, CSS
  overlay head_column (Box v, measure overlay)
     artwork_overlay (Overlay)                          now_playing.rs:172-183
        child   artwork_band  height_request ARTWORK_BAND=280, can_target false
        overlay cloud.widget()  DrawingArea, paints clouds + vertical scrim
        overlay bloom.widget()  BloomArea, blurred cover breathing with the bass
        overlay head (Box v 12, .reprise-now-playing-head padding 22px 18px 0,
                      valign Start, halign Center) → cover_stack (Image 168)
     metadata (Box v 2, .reprise-now-playing-metadata padding 0 18px 16px)
        title / artist / album — three Labels, wrap+ellipsize, each a link
        surface (link_activation::arm_slot, PLAY-12 sensitivity)
stage (Box v 0): track_content, tab_switcher, tab_stack, footer   :286-296
  tab_switcher = adw::InlineViewSwitcher .reprise-now-playing-tabs,
                 set_size_request(1, TAB_SWITCHER_MIN_HEIGHT=50)   :222-229
```

- Z-order today: band → **clouds → bloom** → cover. Title top = band end = 280;
  cover top 22, cover bottom 190. The spec's "gap ≈ 150 in the mockup" is
  this 90 px at 1.67×.
- Scrim: `cover_cloud.rs:81-83` `SCRIM_MID_Y 0.40 / SCRIM_MID_ALPHA 0.15 /
  SCRIM_FULL_Y 0.55` as fractions of the **field** (`field()` :238, height
  440/240·cover, top −60/240·cover), built in `build_scrim()` :700 from
  `accent::sidebar_background_rgb()`, cached by `(theme, dark, field_top,
  field_height)` :290-304, painted after the layers in `draw()` :528-590.
  No horizontal fade exists.
- Bloom: `cover_bloom.rs:39-44` rest/pressure/swell dark 0.06/0.15/0.16
  (max 0.37), light 0.14/0.26/0.24 (max 0.64); `bloom_opacity()` :78-83 has
  no cap. `BLOOM_FULL_STRENGTH_Y = 22.0 + COVER_SIZE` :35 — the head top as a
  literal. `BLOOM_HEIGHT = ARTWORK_BAND` :32.
- Cover CSS `surface_css.rs:22-25`: `border-radius: 12px; box-shadow: inset 0
  0 0 1px @reprise_cover_edge, 0 2px 6px @reprise_cover_shadow`.
  `@reprise_cover_edge`: `theme_tokens.rs:164` (dark, `alpha(@sidebar_fg_color,
  0.12)`) and `:81` (light, `alpha(@window_fg_color, 0.10)`) from
  `tokens.rs:341/344`; `@reprise_cover_shadow` `:165`/`:85` from
  `tokens.rs:346/349` (dark 0, light 0.16).
- Segment control CSS `surface_css.rs:31-42` styles `.reprise-now-playing-tabs`
  (radius `NOW_PLAYING_PILL_RADIUS` = 99px, padding 2, margin `0 18px 12px`)
  and `.reprise-now-playing-tabs button …`. **Measured under Xvfb (libadwaita
  1.9.3): the switcher's tree is `inline-view-switcher > toggle-group >
  (separator | toggle)*` — the toggles are `GtkToggleButton`s with CSS *name*
  `toggle`, so every `… button` rule in that block matches nothing today.**
  libadwaita paints 1 px `separator` nodes between toggles (`.hidden` next to
  the checked one) and gives `toggle-group` its own padding/border, which is
  why the probe measured 32 px for a 26 px toggle inside a 2 px padding.
- Text roles: `PRIMARY_TEXT_ALPHA 0.95`, `SECONDARY_TEXT_ALPHA 0.70`,
  `HINT_TEXT_ALPHA 0.50` (`tokens.rs:8-14`), emitted as
  `@reprise_{primary,secondary,hint}_fg_color` in `theme.rs:284-286`; the
  contrast floor is enforced by `style/panel_contrast.rs` (`PANEL_ROLES`, 13
  selectors, 4.5:1 in both appearances, all three themes).
- Tests that pin today's numbers and will move:
  `now_playing_tests.rs:212` `now_playing_css_defines_the_21a_stage_head_and_glow`
  (hairline string, `padding: 22px 18px 0`), `:289`
  `head_and_pill_match_the_21a_structure` (168, subtitle classes), `:322`
  `npp_18_head_band_keeps_body_text_outside_every_artwork_layer` (overlay order
  `[band, cloud, bloom, head]`), `:242` `npp_11_…view_switcher_and_footer`
  (CSS strings), `cover_cloud_tests.rs:130/142/157` `npc_10/11/12` (scrim stops
  in field fractions; `npc_12` claims the scrim closes *above the cover's
  bottom edge*, which the new fade deliberately no longer does),
  `cover_bloom.rs:363` (opacity model fields), `theme.rs`/`theme_tokens.rs`
  tests asserting the `reprise_cover_edge` define-color lines.
- Gates that bite: Rust files must stay **< 800 lines**
  (`scripts/check-architecture.sh`; `now_playing.rs` 758, `cover_cloud.rs` 726,
  `cover_cloud_tests.rs` 767, `now_playing_tests.rs` 711); every `[active]`
  rule needs a rule-named test (`scripts/check-ux-traceability.sh`); display
  tests carry `#[ignore = "requires a display; run via xvfb-run"]` and run one
  process each (`scripts/check-display-tests.sh`); a `css.contains(…)` test
  proves emission, not the cascade — geometry is asserted on allocations.

## Decisions (grilled with Marvin 2026-09-11 — all seven confirmed)

1. **The artist and the album stay two widgets.** Both are link surfaces today
   (`GO_TO_PLAYING_ARTIST` / `GO_TO_PLAYING_ALBUM`, PLAY-12 sensitivity,
   focusable, own accessible labels). One markup label would drop one reveal
   target or need Pango `<a>` links outside the `link_activation` contract. So
   the one-liner is a horizontal `Box` holding the two labels; the album label
   carries the separator (`" · Album"`). Ellipsizing: GTK's box allocation
   hands a shortfall out in equal shares, so the *longer* of the two yields
   first — with a short artist and a long album (the common case) the album is
   cut exactly as the spec asks; strict "artist first" in every case would need
   a custom layout manager and is not built.
2. **Album tone: 0.70 in dark, 0.65 in light**, as a new named colour
   `@reprise_tertiary_fg_color`. The head-band glow extreme leaves no room for
   a third dark tone under NPP-17's 4.5:1 floor: 0.55 reaches only 3.58:1,
   while 0.70 reaches 4.75:1. In dark, weight 400 and the separator carry the
   quieter album role against the artist's weight 500; light retains the tonal
   step, where 0.65 reaches 4.76:1. Per-appearance alpha has precedent.
3. **Separator colour `@borders`** as the spec says — libadwaita 1.9 defines it
   (`color-mix(currentColor 15 %)`, 50 % in high contrast), so it follows the
   appearance without a literal. `@reprise_hairline` (6 %/9 %) is the fainter
   in-house alternative.
4. **NPP-2 is edited in place**, as the spec asks: the rule keeps its meaning
   (layout from top) with new values; NPP-17's mention of the cover's inset
   hairline is dropped in the same edit. No new rule ID. The rulebook reserves
   `[replaced by …]` for a change of meaning (P-5 → BROWSE-6, NPP-10 →
   NPP-13); rewriting an active rule's numbers in place is the repo's
   practice (8 of the last 25 rulebook commits, e.g. #733, #824).
5. **Shadow through the token, not a literal**: `0 12px 30px
   @reprise_cover_shadow` with both appearances at 0.42, so the panel CSS keeps
   referencing only named colours (NPP-17's fixed-foreground sweep).
6. **File splits to stay under 800 lines**: `now_playing_head.rs` (head
   construction), `cover_scrim.rs` (+ `cover_scrim_tests.rs`),
   `now_playing_head_tests.rs`. Pure moves first, changes after.
7. **The fade closes at the title, not above the cover's bottom edge.**
   `npc_12` today records a deliberate earlier call: the scrim must be fully
   opaque *before the cover ends* (today from y ≈ 127), so nothing moves below
   the cover at all. The spec's stops put the scrim at ≈ 0.68 at the cover's
   bottom edge (234) and 1.0 only at 268 — the clouds stay visible (≈ 32 %)
   in the 34 px between cover and title, fading to nothing exactly at the
   title's top edge. That is what variant 4b shows and what NPP-18 literally
   asks ("fades to zero before that boundary"); it overturns the stronger
   earlier claim on purpose. `npc_12` is rewritten to the new contract.

## Target geometry (logical px, panel 300 wide)

```
0    band top                       scrim α 0
50   cover top   NOW_PLAYING_HEAD_TOP
178.7 two thirds to the title       scrim α 0.15
234  cover bottom (184)
268  band end = title top           scrim α 1.0     NOW_PLAYING_ARTWORK_BAND
     title 15 px bold, then "Artist · Album" 12 px, metadata padding 0 18 16
     (the 16 px bottom padding of `metadata` is the owner of the gap above
     the segment control — nothing else adds to it)
+16  segment control 30 px, stage content width − 2·18 (margins 18; the
     spec's "262" is 300 − 36 miscounted — the contract is the derived width,
     the stage's 1 px border-left leaves whatever it leaves), radius 7 / 5
+20  rule 1 px, @borders, run-out 34 px at both ends, full stage width
+8   tab content
x: panel colour 100 % at x=0 → 0 at x=66 (0.22 · width)
```

## Tasks

### 1. Tokens (`style/tokens.rs`)

Add, in the NOW_PLAYING block with doc comments naming the spec:
`NOW_PLAYING_HEAD_TOP: i32 = 50`, `NOW_PLAYING_COVER_TO_TITLE: i32 = 34`,
`NOW_PLAYING_SEGMENT_HEIGHT: i32 = 30`, `NOW_PLAYING_SEGMENT_RADIUS: &str =
"7px"`, `NOW_PLAYING_SEGMENT_INNER_RADIUS: &str = "5px"`,
`NOW_PLAYING_LIST_RULE_RUN_OUT: i32 = 34`, `NOW_PLAYING_LIST_RULE_ABOVE: i32 =
20`, `NOW_PLAYING_LIST_RULE_BELOW: i32 = 8`, `NOW_PLAYING_LEFT_FADE_SHARE: f64
= 0.22`, `TERTIARY_TEXT_ALPHA_DARK: f64 = 0.70`, `TERTIARY_TEXT_ALPHA_LIGHT:
f64 = 0.65`.
Change: `NOW_PLAYING_COVER_SIZE` 168 → 184; `NOW_PLAYING_ARTWORK_BAND` becomes
the derived `NOW_PLAYING_HEAD_TOP + NOW_PLAYING_COVER_SIZE +
NOW_PLAYING_COVER_TO_TITLE` (= 268) with a comment that the band ends where the
title begins; `COVER_SHADOW_DARK_ALPHA` and `COVER_SHADOW_LIGHT_ALPHA` → "0.42".
Remove: `NOW_PLAYING_PILL_RADIUS`, `COVER_EDGE_DARK_ALPHA`,
`COVER_EDGE_LIGHT_ALPHA`. Cloud/bloom caps stay in their modules.

### 2. Theme colours (`style/theme.rs`, `style/theme_tokens.rs`)

- Emit `@define-color reprise_tertiary_fg_color alpha({fg}, {tertiary_alpha})`
  next to primary/secondary (`theme.rs:284-286`), dark 0.70 / light 0.65.
- Drop both `reprise_cover_edge` define-colors; set both `reprise_cover_shadow`
  alphas from the 0.42 tokens.
- Update the tests in `theme.rs` (`:714-722` expected strings, the `#891`
  edge tests) and `theme_tokens.rs`: an assertion that the hairline exists
  becomes an assertion that `reprise_cover_edge` is **absent** from both
  appearances' CSS; the shadow assertions take 0.42.
- `panel_contrast.rs`: `rendered_foreground` learns
  `@reprise_tertiary_fg_color` (alpha by appearance — thread the appearance
  through from the loop at `:392`), and `PANEL_ROLES` (a fixed-length array,
  `[PanelRole; 13]` → 14) gains `.reprise-now-playing-album` at 4.5. The
  `.reprise-now-playing-subtitle` entry (`:38`) and the match arm at `:430`
  → `.reprise-now-playing-artist`.

### 3. Head extraction (pure move) — `now_playing_head.rs`

Move the construction of cover/outgoing cover/cover_stack, the three labels,
`metadata`, `glow`, `artwork_band`, `artwork_overlay`, `bloom`, `cloud`,
`head`, `head_column`, `head_group` (`now_playing.rs:92-190`) into
`pub(super) fn build_head(...) -> HeadWidgets` in a new
`now_playing_head.rs`; `build_widgets_for_session` consumes the struct and
`PanelWidgets` keeps its field names so every existing test and
`now_playing_effects.rs` compile unchanged. Commit this move on its own before
any behaviour changes (reviewable diff). `now_playing.rs` must end well under
800 lines (target ≤ 700).

### 4. Stacking and head geometry (`now_playing_head.rs`, `surface_css.rs`)

- Overlay order: `add_overlay(bloom.widget())` **before**
  `add_overlay(cloud.widget())`; rewrite the comment: band → bloom → clouds
  (which own the scrim) → cover.
- `.reprise-now-playing-head { padding: {NOW_PLAYING_HEAD_TOP}px 18px 0; }`
  (token-interpolated, no literal 22/50 in the CSS string).
- Cover: `border-radius: {RADIUS_SURFACE}; box-shadow: 0 12px 30px
  @reprise_cover_shadow;` — no inset ring. `GtkImage` stays (clipping to the
  radius is already true for it).
- Update the comment on `head.set_valign(Start)` (no "280 px", no "22 px").
- `cover_bloom.rs:35`: `BLOOM_FULL_STRENGTH_Y = NOW_PLAYING_HEAD_TOP as f64 +
  COVER_SIZE` — the literal goes.

### 5. The scrim moves to the head geometry and gains the left fade — `cover_scrim.rs`

- Extract `scrim_alpha`, `build_scrim`, `paint_scrim`, `cached_scrim`,
  `ScrimCacheKey`, `ScrimCache`, `scrim_cache_needs_rebuild`, `STOPS` and the
  three stop constants from `cover_cloud.rs` into `cover_scrim.rs` (pure move,
  own commit), tests into `cover_scrim_tests.rs` (`npc_10/11/12` move with
  them; `cover_cloud_tests.rs` keeps the drift/clock/fade tests).
- Vertical fade, in **band pixels**, not field fractions: `title_top =
  ARTWORK_BAND as f64`; stops `(0.0 → 0.0)`, `(2/3 · title_top → 0.15)`,
  `(title_top → 1.0)`, clamped both sides. Keep the 24-stop cairo gradient but
  span it `0 … title_top`. Constants: `SCRIM_MID_SHARE = 2.0 / 3.0`,
  `SCRIM_MID_ALPHA = 0.15`; `SCRIM_FULL_Y` goes (the full stop *is* the title
  top). `scrim_alpha(y_px, title_top) -> f64`.
- Horizontal fade: a second cairo `LinearGradient` from `x = 0` (panel colour,
  α 1.0) to `x = NOW_PLAYING_LEFT_FADE_SHARE · width` (α 0.0), linear, same
  colour, painted after the vertical one over the whole band. Pure
  `left_fade_alpha(x_px, width) -> f64`.
- Cache key becomes `(theme, dark, width)`; the vertical gradient no longer
  depends on the field. `draw()` in `cover_cloud.rs` calls
  `cover_scrim::paint(cr, inner, dark, width, band)` after the cloud layers.
- Order of painting inside the cloud widget is unchanged: layers, then the
  fades. Because the cloud widget now sits **above** the bloom, the fades cover
  bloom and clouds alike — one owner, as the spec asks.
- Tests (`cover_scrim_tests.rs`, pure): `npc_10` → the three stops at 0,
  178.67, 268 for `title_top = 268`, clamped outside; `npc_11` monotonic over
  0…title_top; `npc_12` → rewritten to the new contract: `scrim_alpha(title_top)
  == 1.0` and `title_top == NOW_PLAYING_ARTWORK_BAND` (the band ends where the
  text starts — NPP-18's actual claim); new `npc_13_the_left_fade_reaches_the_
  list_edge_at_a_fifth_of_the_panel`: α 1.0 at x 0, 0.5 at 33, 0.0 at 66 and
  beyond, for width 300; and the fade width scales with `width`.

### 6. Bloom cap (`cover_bloom.rs`)

`bloom_opacity_model` gains `cap` (dark 0.35, light 0.60) and
`bloom_opacity` returns `(rest + …).min(model.cap)`. Pure tests:
`bloom_opacity(1.0, 1.0, true) == 0.35`, `bloom_opacity(1.0, 1.0, false) ==
0.60`, `bloom_opacity(0.0, 0.0, true) == 0.06` (rest untouched),
`bloom_opacity(1.0, 0.0, true) == 0.21` (under the cap nothing moves). Extend
`light_bloom_is_stronger_while_dark_keeps_its_original_opacity_model` with the
cap fields. Breath (`REST_SCALE`, `SCALE_PER_SWELL`) untouched.

### 7. Title block (`now_playing_head.rs`, `now_playing_effects.rs`, `surface_css.rs`, `panel_state.rs`)

- Title: `wrap(false)`, `ellipsize(End)`, `single_line_mode` not needed; keeps
  `.reprise-now-playing-title` (15 px, 700) and its link slot.
- Artist label → class `reprise-now-playing-artist`, album label → class
  `reprise-now-playing-album` (both `wrap(false)`, `ellipsize(End)`, `xalign
  0.5` irrelevant inside the row). Both keep `AccessibleRole::Link`, their
  `arm_slot` and `relabel` wiring, and the PLAY-12 sensitivity loop.
- New horizontal `gtk::Box` `subtitle_row` (spacing 0, `halign Center`,
  class `reprise-now-playing-subtitle-row`) inside `metadata` after the title;
  `metadata` spacing stays 2.
- Pure `fn album_label_text(artist: &str, album: &str) -> String` in
  `panel_state.rs`: `"·\u{00A0}Album"` preceded by a normal space when the
  artist is non-empty, plain `"Album"` otherwise; empty album → empty string.
  `render_track` (`now_playing_effects.rs:36-48`) sets the album label from
  it; the visibility rules stay (`artist` hidden when empty, `album` hidden
  when empty). External sessions keep the joined `presentation.subtitle` in
  the artist label and an empty album. Unit tests for the four
  empty/non-empty combinations and for a `&` in the album (plain text, no
  markup — nothing to escape, assert that `use_markup` is false).
- CSS: `.reprise-now-playing-artist { color: @reprise_secondary_fg_color;
  font-size: {NOW_PLAYING_SUBTITLE_SIZE}; font-weight: 500; }`
  `.reprise-now-playing-album { color: @reprise_tertiary_fg_color; font-size:
  {NOW_PLAYING_SUBTITLE_SIZE}; font-weight: 400; }`. Remove
  `.reprise-now-playing-subtitle`. No box, no backdrop, no text-shadow (none
  exists today — verify nothing else installs one: grep `text-shadow` in
  `ui/`).

### 8. Segment control (`now_playing.rs`, `surface_css.rs`)

- `TAB_SWITCHER_MIN_HEIGHT` → `tokens::NOW_PLAYING_SEGMENT_HEIGHT` (30).
- CSS, written against the real nodes:
  `.reprise-now-playing-tabs { background-color: alpha(@sidebar_fg_color,
  {PILL_BG_ALPHA}); border-radius: {SEGMENT_RADIUS}; padding: 2px; margin: 0
  18px 0; }`
  `.reprise-now-playing-tabs toggle-group { padding: 0; border: none;
  background: none; box-shadow: none; min-height: 0; border-radius: 0; }`
  `.reprise-now-playing-tabs separator { min-width: 2px; background: none;
  opacity: 0; }` (the 2 px gap; libadwaita's `.hidden` toggling becomes moot)
  `.reprise-now-playing-tabs toggle { background: transparent; border: none;
  box-shadow: none; border-radius: {SEGMENT_INNER_RADIUS}; min-height: 0;
  padding: 0; color: @reprise_secondary_fg_color; }`
  `.reprise-now-playing-tabs toggle:checked { background-color:
  @reprise_tab_active_bg; box-shadow: 0 1px 2px @reprise_tab_active_shadow;
  color: @reprise_primary_fg_color; }`
  Delete every `… button` rule in the block (dead today).
- The switcher's allocated height must come out at exactly 30 with the
  toggles at 26: if libadwaita's own toggle padding pushes it, pin `toggle {
  min-height: 26px }` — the display test is the contract, the CSS is the
  means. The bottom margin moves to the rule (task 9).

### 9. Rule before the list (`now_playing.rs`, `surface_css.rs`)

- A `gtk::Box` (v, 0) `list_rule`, class `reprise-now-playing-list-rule`,
  `can_target(false)`, `height_request(1)`, appended to `stage` between
  `tab_switcher` and `tab_stack`; exposed on `PanelWidgets` as `list_rule`.
- CSS: `.reprise-now-playing-list-rule { min-height: 1px; margin:
  {LIST_RULE_ABOVE}px 0 {LIST_RULE_BELOW}px; background-image:
  linear-gradient(to right, alpha(@borders, 0), @borders {RUN_OUT}px, @borders
  calc(100% - {RUN_OUT}px), alpha(@borders, 0)); }`. If GTK rejects the
  `calc()` stop (the `now_playing_css_parses_without_gtk_errors` display test
  says so), use percentage stops `11.33% / 88.67%` of the 300 px panel with a
  comment deriving them from the token; if it rejects `alpha(@borders, 0)`
  (a `color-mix` colour inside `alpha()`), use `transparent` — GTK
  interpolates gradients premultiplied, so no grey creeps in.
- The tab content's own paddings stay as they are: the 8 px is measured from
  the rule to the `tab_stack`'s top edge.

### 10. Rule text (`docs/ux-rules.md`)

NPP-2 (`:2235-2242`) becomes — keeping id, status, level tag and the P-1 and
NPP-17 sentences: "Layout from top: cover 184 px (radius 12, shadow, no
hairline) 50 px from the top → 34 px → title 15 px bold → „Artist · Album"
on one line, 12 px, the artist at the secondary tone (weight 500), separator
and album at the tertiary tone → **segment control** 30 px, radius 7 px /
5 px inside, the stage's content width less the 18 px margins on both sides
(segments, no tab-bar widget) → 20 px → a 1 px rule
in the border tone running out over 34 px at both ends → 8 px → tab content →
footer 10.5 px 35 %, whose content is provided by the active tab. …". In
NPP-17 drop "the cover's inset hairline" from the list of surface washes.
NPP-18 unchanged (the fade reaching 100 % at the title's top edge is exactly
its claim).

### 11. Tests that move, and the new display tests

Update in place: `now_playing_css_defines_the_21a_stage_head_and_glow`
(no hairline, new shadow string, `padding: 50px 18px 0`),
`head_and_pill_match_the_21a_structure` (184, the two new classes, the
`list_rule` sits between switcher and stack), `npp_11_…` (new selector
strings, no `button`), `npp_18_head_band_keeps_body_text_outside_every_
artwork_layer` (order `[band, bloom, cloud, head]`, `ARTWORK_BAND == HEAD_TOP
+ COVER + COVER_TO_TITLE`). **Every test that reads the album label's text
moves with the separator:** `now_playing_tests.rs:494-502` (`"Loaded album"`
→ `album_label_text("Loaded artist", "Loaded album")`, and the empty case),
`now_playing_external_tests.rs:127-128` (external: artist carries the joined
subtitle, album stays empty — unchanged, but re-read it), plus the `:516-517`
link-surface list. Before touching them, grep the crate for
`widgets.artist`, `widgets.album`, `now-playing-subtitle` and list every hit
in the commit message of the title-block commit.

New file `now_playing_head_tests.rs` (declared from `now_playing_head.rs` with
`#[path]` like the others), all `#[ignore = "requires a display; run via
xvfb-run"]`, built on the existing `test_widgets` helper, the widgets placed in
a realized 300 px-wide window and measured with `compute_bounds` against the
stage:

- `npp_2_the_cover_sits_fifty_px_down_and_the_title_thirty_four_below_it`:
  cover top edge = 50, cover 184², metadata top = 268 (= 50 + 184 + 34); the
  band's allocated height = 268.
- `npp_2_the_segment_control_is_thirty_px_tall_with_two_px_gaps`: switcher
  height 30 and width = the stage's content width − 36 (derived in the test
  from the stage's own allocation, never a literal); each `toggle` node 26
  tall; consecutive toggles 2 px apart; `toggle-group` contributes no extra
  padding.
- `npp_2_a_hairline_with_run_out_separates_the_switcher_from_the_list`:
  `list_rule` is 1 px tall, 20 px below the switcher's bottom edge, 8 px above
  the `tab_stack`'s top edge, as wide as the stage's content.
- `npp_18_the_fades_hand_the_title_calm_ground_and_keep_the_list_edge_clean`
  — **not a window render.** The cloud layer's `draw(cr, width, height,
  &inner)` is a free function over a cairo context: paint it into a
  300 × 268 `cairo::ImageSurface` with a saturated 64 × 64 `MemoryTexture`
  cover set through `set_cover`, the clock held (`set_pinned(true)`: a
  standing clock counts the fade as arrived, `draw()` :555-560), then read
  pixels. No renderer, no realize, no Xvfb variance — the production paint
  path, exactly. Expectations come from the pure functions, not magic
  numbers: at `(0, 0)` `left_fade_alpha` is 1.0 → panel colour exactly; on
  the last row `(150, 267)` `scrim_alpha(267.5, 268)` ≈ 0.996 → within ±4 of
  the panel colour; at `(150, 120)` vertical α ≈ 0.10 and horizontal 0 → at
  least one channel ≥ 20 away from the panel colour (the clouds are there).
  Control arm inside the test: the same surface painted with the fades
  skipped (a `paint_layers_only` seam that the production `draw` also calls
  — the fades are then one extra call, not a knob) shows `(0, 0)` and
  `(150, 267)` **off** the panel colour. This is the direct proof of "text on
  calm ground, nothing bleeds into the list edge". If `set_cover` turns out
  to need a display for `blurred_surface`, the test becomes display-gated
  like its neighbours — still the same offscreen surface, never a window
  render. The overlay *order* is the structural `npp_18_head_band_…` test;
  the two together carry the contract.
- Keep `now_playing_css_parses_without_gtk_errors` green (it is the parse
  guard for the gradient and `calc()`).

## Verification (Codex, in the worktree)

1. `cargo fmt --all` and `cargo clippy -p reprise-gnome --all-targets -- -D
   warnings` (the repo's lint baseline; do not add allows).
2. `cargo test -p reprise-gnome now_playing` and `cargo test -p reprise-gnome
   style::` for the pure tests (scrim, bloom cap, album text, tokens, theme,
   panel_contrast).
3. Every new or changed display test **individually**, with the gate's exact
   environment (never a bare `cargo test --ignored` with a filter — GTK
   initialises once per process and a bundle aborts in `gtk_init`):
   ```
   GDK_BACKEND=x11 WAYLAND_DISPLAY= GSK_RENDERER=cairo GTK_USE_PORTAL=0 \
   GIO_USE_VFS=local REPRISE_AUDIO_SINK=fakesink \
   XDG_CONFIG_HOME=$(mktemp -d) XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) \
   dbus-run-session -- xvfb-run -a \
     cargo test -p reprise-gnome -- --ignored --exact --nocapture <full test path>
   ```
   for: the four new tests, `npp_18_head_band_keeps_body_text_outside_every_
   artwork_layer`, `npp_14_tabs_are_always_icon_only_with_installed_labeled_
   symbols`, `now_playing_css_parses_without_gtk_errors`,
   `npp_17_the_panel_takes_its_foreground_from_the_appearance` (if display-
   gated), and every display test in `cover_cloud_tests.rs`.
4. `scripts/check-architecture.sh`, `scripts/check-ux-traceability.sh`,
   `scripts/check-motion-tokens.sh`, `scripts/check-gnome-idioms.sh`,
   `scripts/check-ai-hygiene.sh`, `scripts/check-accessibility-semantics.sh`
   — each directly; record any that the sandbox cannot run.
5. Do **not** run `scripts/check-merge-readiness.sh` in full (its rule-owned
   display stage takes 35–45 min and is the reviewer's job below).

## Verification (reviewer, after `/code`)

- Detached run of `scripts/check-display-tests.sh --rule-named` in the
  worktree with a wake lock, control arm = the same script on a clean
  `origin/dev` worktree if anything unrelated is red.
- Screenshot pair dark/light from the Xvfb harness
  (`~/.claude/…/memory/reprise-screenshot-harness.md` recipe) for the eye —
  not the proof.

## Out of scope

Android (`NowPlayingScene.kt`), the clouds' drift and alpha numbers (#927),
the NPP-3 glow, the footer, the Up Next list's own paddings, the lyrics and
visualizer tabs, the mini player.

## Parallelität

**Not cut — one strand.** Every candidate split shares files that both halves
must edit: `style/tokens.rs` (the head-top and band tokens feed the bloom, the
scrim *and* the CSS), `now_playing.rs`/`now_playing_head.rs` (the overlay-order
swap and the head/label/segment/rule work live in the same function), the
tests in `now_playing_tests.rs`, and `docs/ux-rules.md`. The band height is one
derived contract (`HEAD_TOP + COVER + COVER_TO_TITLE`) that the scrim, the
bloom mask, the head padding and the geometry tests all read; a strand that
owned "artwork band" would still have to wait for the strand that owns the
tokens before its tests compile, which is a merge order, not parallelism. The
work is ~11 tasks in one worktree; task 3 and the first half of task 5 are pure
moves committed on their own so the behaviour diff stays reviewable.

Post-merge cross-checks: none — with one strand every comparison happens in
the branch.
