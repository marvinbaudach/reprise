---
slug: the-mini-player-follows-the-appearance
worktree: /home/marvin/Projects/reprise-the-mini-player-follows-the-appearance
branch: feature/the-mini-player-follows-the-appearance
phase: shipped
codex_session:
created: 2026-09-09
---
# The mini player follows the appearance

## The report

> „die miniview sollte auch im gleichen style und mode erscheinen wie der große
> player. hier lightmode"

## What is actually wrong — measured, not eyeballed

The screenshot's pixels were sampled directly
(`~/.claude/image-cache/71755909-…/1.png`, 908×200, the mini window over a black
backdrop). WCAG relative luminance, three decimal places kept:

| Region | Colour | Against | Ratio | |
| --- | --- | --- | --- | --- |
| Card surface | `#222222` | — | — | `rgba(34, 34, 34, 0.92)` over black |
| Title glyph (`Pretty Lies`) | `#1a1c1f` | card | **1.07** | invisible |
| Artist glyph (`If Not for Me`) | `#1d1e20` | card | **1.05** | invisible |
| Play-button fill | `#006f6c` | — | — | light-derived accent |
| Play glyph | `#ffffff` | fill | 6.02 | ok |

Two of those numbers settle the diagnosis without any further reading:

- `#1a1c1f` **is** `Theme::PerpetualRain.light_palette().fg`, byte for byte.
- 6.02:1 is the exact figure `player_bar_layout.rs` records in its PLAY-16
  comment for the *light* appearance ("in light, the playback accent follows the
  derived accent colour and raises the ratio to 6.02:1").

So the mini window **does** receive `theme_css`, and it **is** in the light
appearance. Every themed value in the card is already correct. One hardcoded
literal is not.

**The headline is not "wrong grey". It is that in the light appearance the mini
player's title and artist sit at 1.05–1.07:1 and cannot be read at all.** What
looks like light text in the screenshot is the waveform; the glyphs differ from
the card by 17 of 765 possible channel steps. A row profile at a normal
`Σ|Δ| > 25` threshold finds *nothing* above the waveform — the text only appears
at `> 4`. Worth remembering: a sweep looking for "where is the text" with a
sensible threshold concludes there is none.

## Cause

`crates/reprise-gnome/src/ui/compact/compact_player_layouts.rs:164` `mini_css()`:

```
.mini-player-card  { background-color: rgba(34, 34, 34, 0.92);
                     border: 1px solid alpha(white, 0.09); }
.mini-player-cover { box-shadow: inset 0 0 0 1px alpha(white, 0.08); }
.mini-player-artist{ color: alpha(@window_fg_color, 0.6); }
```

Three dark-only literals sitting next to a foreground that is a theme token.

`abb205cef9` ("The light appearance gets its own edges", #891) converted the
whole GTK frontend to appearance-aware tokens and **skipped this file** —
`compact_player_layouts.rs` does not appear in that commit's file list. This
plan finishes that commit's job for the one surface it missed.

## The mechanism — the constraint that decides the shape

`mini_css()` is joined into `app_css()` (`style/mod.rs:147`), which is built
**once** and never learns `is_dark`. `theme::theme_css(theme, is_dark, source)`
is the only thing rebuilt per appearance (`style/mod.rs:227`).

Therefore: **a `@define-color` emitted by `theme_css` is the only channel.** A
Rust-side `libadwaita::StyleManager::is_dark()` branch inside `mini_css()` would
compile, pass its own unit test, and do nothing at runtime — the string is built
before the appearance is known and never rebuilt.

That constraint buys the property the whole #891 branch rests on: **dark
identity is provable by construction.** Each token's dark branch reproduces the
literal it replaces byte for byte, so text identity of the generated CSS *is*
pixel identity in the dark appearance. The proof lives in `theme_tokens.rs`'s
`dark_appearance_tokens_reproduce_every_replaced_literal`.

## The change — four tokens

Added to `ThemeTokens` (`style/theme_tokens.rs`), fed from new constants in
`style/tokens.rs`, emitted by `theme_css` (`style/theme.rs`), consumed by
`mini_css()`.

| Token | dark (byte-identical to today) | light |
| --- | --- | --- |
| `reprise_mini_card_bg` | `rgba(34, 34, 34, 0.92)` | `alpha(@headerbar_bg_color, 0.92)` |
| `reprise_mini_card_edge` | `alpha(white, 0.09)` | `rgba(0, 0, 6, 0.14)` |
| `reprise_mini_cover_edge` | `alpha(white, 0.08)` | `alpha(#000000, 0.10)` |
| `reprise_mini_artist_fg` | `alpha(@window_fg_color, 0.6)` | `alpha(@window_fg_color, 0.70)` |

### Why no existing token can be reused

`reprise_hairline` is `rgba(255, 255, 255, 0.06)` in dark and
`reprise_hairline_strong` is `0.07`; the mini card's edge is `0.09`.
`reprise_cover_edge` is `alpha(@sidebar_fg_color, 0.12)`; the mini cover's inset
is `alpha(white, 0.08)`. Adopting a near-miss would move dark pixels for no
light-mode gain — precisely what the identity property forbids. New tokens, dark
branches equal to the literals they replace.

The light edge takes `0.14` — `PILL_BORDER_LIGHT`'s value, not
`HAIRLINE_STRONG_LIGHT`'s `0.11` — because that is the matching *role*: the mini
card is a floating surface over an arbitrary desktop, exactly what
`PILL_BORDER_LIGHT` was tuned for ("its white dark twin disappears on the lifted
white pill surface"), while the hairline values were tuned for rules inside a
table.

### Why the artist foreground needs its own light branch

The card is glass (`0.92`) on a transparent toplevel, so its composite depends
on whatever desktop is behind it. Worst case over the three light palettes ×
three backdrops (white / mid-grey / black), artist = `alpha(fg, a)` over the
card:

| light card surface | a = 0.60 | a = 0.65 | a = 0.70 |
| --- | --- | --- | --- |
| `@headerbar_bg_color` | 3.95 – 4.24 | 4.55 – 4.95 | **5.27 – 5.80** |
| `@sidebar_bg_color` | 4.00 – 4.29 | 4.62 – 5.02 | 5.36 – 5.89 |
| `@card_bg_color` | 4.23 – 4.50 | 4.93 – 5.30 | 5.78 – 6.29 |

**No surface clears 4.5:1 at today's `0.60`.** `0.70` clears everywhere. The
title, which paints full `@window_fg_color`, lands at 14.4:1.

### Why `@headerbar_bg_color` and not `@card_bg_color`

`@headerbar_bg_color` is what the big player bar paints
(`player_bar_layout.rs`, `.player-bar-surface`), so it is the literal reading of
"same style as the big player" — the request. It is also the tightest row of the
table; `@card_bg_color` would carry ~0.5 more headroom, and this codebase does
re-pick light surfaces by role rather than mirroring the dark one
(`PILL_BG_DARK = @sidebar_bg_color` vs `PILL_BG_LIGHT = @card_bg_color`). The
request wins: `0.70` puts the worst case at 5.27:1, comfortably over AA, so the
headroom argument buys nothing that is actually needed.

### The card is the wrong ground for four things, not two

The mini player builds its waveform with `WaveformSeek::new_mini()` — the *same*
widget and the same `WaveformAppearance::current()` path as the big player bar.
In the light appearance that appearance object sets:

- `spectral_background = theme.light_palette().view_bg` (`#fafbfc`), and
  `adjust_spectral` then **darkens** every spectral bar until it clears 3:1
  against that near-white ground;
- `fallback_unplayed = theme.light_palette().fg` — near-black coming bars.

Both are painted today onto a near-black card. So the light appearance's
title, artist, spectral bars and fallback bars are all composed against a ground
that is not there. The screenshot shows it: those mid-saturation pink/purple
bars are already the *darkened* light-mode variant.

This settles the surface choice rather than merely supporting it. The waveform
assumes a near-white ground; `@headerbar_bg_color` at `0.92` lands at `#e8eaed`
over a light desktop. Against the assumed `#fafbfc` that is ~5 % darker, so a bar
tuned to clear 3:1 on `view_bg` clears slightly *more* on the card — the
mismatch runs in the safe direction, and it is the **identical** mismatch the
big player bar already ships with, since it paints `@headerbar_bg_color` under
the same waveform. Matching the big player therefore reproduces the shipping
situation exactly, which is what "im gleichen style und mode wie der große
Player" asks for.

## Tasks

**1 — `style/tokens.rs`: eight constants.**
`MINI_CARD_BG_DARK` / `_LIGHT`, `MINI_CARD_EDGE_DARK` / `_LIGHT`,
`MINI_COVER_EDGE_DARK` / `_LIGHT`, `MINI_ARTIST_ALPHA` / `MINI_ARTIST_LIGHT_ALPHA`.
Each with the doc comment style of its neighbours, saying why the dark twin
cannot be reused in light. Follow the file's existing convention exactly.

**2 — `style/theme_tokens.rs`: four fields.**
`mini_card_bg`, `mini_card_edge`, `mini_cover_edge`, `mini_artist_fg` on
`ThemeTokens`, selected in `for_appearance(is_dark)` with the `select(dark,
light)` helper the file already uses. All four are complete CSS *colour values*,
never bare alphas — a GTK named colour holds a colour, and every existing field
in this struct is a whole value.

**3 — `style/theme.rs`: four `@define-color` lines** in `theme_css`, placed with
the other `reprise_*` definitions and *after* `@headerbar_bg_color` and
`@window_fg_color` are defined in the same string.

**4 — `compact/compact_player_layouts.rs`: consume them.**
`background-color: @reprise_mini_card_bg`, `border: 1px solid
@reprise_mini_card_edge`, `box-shadow: inset 0 0 0 1px
@reprise_mini_cover_edge`, `color: @reprise_mini_artist_fg`. Nothing else in
`mini_css()` changes — the play button rule in particular is untouched.

**5 — Move the dark-identity proof, do not delete it.**
`mini_1_card_css_matches_frame` (`compact_player_layouts.rs:242`) asserts the
literal `rgba(34, 34, 34, 0.92)` lives in `mini_css()`. After tokenising it will
not. Replace that one line with an assertion on the token name, and add the four
definitions to the `definitions` array in `theme_tokens.rs`'s
`dark_appearance_tokens_reproduce_every_replaced_literal`, in the same string
form as the sixteen already there. The dark guarantee must not weaken by one
assertion.

**6 — Give `mini_artist_contrast_on_tint` a light arm.**
Today it hardcodes `bg = 34.0/255.0` and `ARTIST_ALPHA = 0.6`. It becomes two
arms: the dark arm keeps today's numbers verbatim, and the light arm sweeps
`Theme::all()` × `light_palette()` × a **modelled** backdrop set — white,
mid-grey (`#808080`) and black — compositing `headerbar_bg` at `0.92` and the
artist at `MINI_ARTIST_LIGHT_ALPHA`, asserting ≥ 4.5:1 throughout. This test is
what *chose* the alpha, so it must be able to fail if the alpha is lowered again.

Say in the test's comment that those three are a modelled range, not an
exhaustive one: the card is `0.92` glass over an **arbitrary wallpaper**, so no
finite sweep is complete — the three bracket the luminance extremes and the
midpoint. An opaque card would need none of this. The dark arm's honesty is the
model it states out loud ("on a dark desktop ≈ #222"); the light arm owes the
same sentence, or the next person to touch the alpha will read three samples as
a proof. Reuse `style::color_math` (`composite`, `contrast_ratio`,
`parse_hex_rgb`) rather than re-deriving luminance by hand — `contrast_ratio`
owns the shared `relative_luminance` call, whose narrower `ui::style`
visibility deliberately does not reach this compact-player sibling.

**7 — Assert the generated light CSS actually parses.**
One test running `theme_css(theme, false, source)` through
`style::mod::css_parse_errors` for every theme × accent source, asserting no
errors. `alpha()` over a named colour inside `@define-color` is already proven in
production (`@define-color reprise_hover_bg alpha(@accent_bg_color, 0.10);`), so
this is expected to pass on the first run — it is there because *this file* has
the scar: five declarations carried `!important` until 2026-08-03, GTK4's parser
rejected the value as junk and dropped all five, and the fix shipped doing
nothing for months. A silent parse drop is the one failure mode that looks
exactly like success.

## Verification

Run in the worktree, output to `$SCRATCH/*.log`, answered by `grep`:

- `cargo test -p reprise-gnome --bin reprise style::` and `compact::` — the
  token, identity, contrast and parse tests. This crate has no library target;
  its unit tests live on the `reprise` binary target.
- `cargo clippy -p reprise-gnome --all-targets -- -D warnings`.
- `cargo fmt --check`.

**Control arm, mandatory.** The claim is "dark is unmoved, light is fixed". Both
halves need evidence:

- *Dark unmoved:* `dark_appearance_tokens_reproduce_every_replaced_literal`
  passing with the four new entries **is** the proof, by the text-identity
  argument above. No screenshot needed for this half.
- *Light fixed:* compute the light composites in the test (task 6) and assert the
  ratios. A screenshot is optional colour, not the evidence.

Do **not** claim the fix works from a passing build. The failing state was
1.07:1; the test that goes from red to green must be the contrast one.

## Non-goals

- Android's mini player (`LibraryFrame.kt:255`) already paints
  `MaterialTheme.colorScheme.surfaceContainer` and follows the appearance
  correctly. Out of scope.
- No geometry, layout, spacing or radius changes. `MINI_WIDTH`/`MINI_HEIGHT` and
  the frame-1e padding stay exactly as they are.
- The play button keeps the playback accent and its white glyph — PLAY-16 is a
  recorded, deliberate exception and `panel_contrast.rs`'s
  `play_16_the_play_buttons_keep_the_playback_accent_and_white_glyph` enforces
  it against `mini_css()`. Do not "fix" it.

## Things that will bite

- **`.mini-player-card` is not in `PANEL_ROLES`.** Only the PLAY-16 test reads
  `mini_css()` from `panel_contrast.rs`. Do not add it to `PANEL_ROLES` while
  you are in there: `rendered_foreground` cannot parse `alpha(@window_fg_color,
  0.6)` and would panic on an unsupported alpha colour.
- **`fixed_foregrounds` only flags `color:` declarations.** Its sweep over the
  composed `app_css()` does reach `mini_css()`, but a border value such as
  `alpha(white, 0.09)` is not caught because the guard filters on
  `property == "color"`. The `.mini-player-play` rule is exempted by selector,
  alongside `.player-bar-play` and `.reprise-build-badge`, so renaming it fails
  loudly instead of silently widening the exemption.
- **`concat!("rgba(255, ", "255, 255, …)")` in `tokens.rs` is deliberate** —
  it keeps the literal out of source-text scans. The new constants contain no
  `255, 255, 255` and need no such trick; do not add one, and do not "clean up"
  the existing ones.
- **A light drop shadow was considered and rejected.** #891 established the
  pattern (`COVER_SHADOW_LIGHT_ALPHA = 0.16` where the dark twin is `alpha(#000000, 0)`),
  but the geometry forbids it: `CARD_MARGIN` is zero, so `MINI_WIDTH + 2 *
  CARD_MARGIN` leaves no room outside the card to render a shadow. It is also out
  of scope because the request is "same style as the big player", and the big
  player bar carries no shadow either; `box-shadow: none` on the card is existing
  deliberate behaviour from the MINI-1/MINI-2 CSD-halo fight; and there is **no
  measurement** showing the `0.14` light edge is insufficient separation.
  Inventing one would move pixels nobody has measured. If the light card reads
  flat in practice, that is a follow-up with a measurement attached — not a
  guess folded into this change. Reviewers: this is deliberate, not an oversight.
- **Base is `origin/dev` @ `0970978e70`.** `theme_tokens.rs` does not exist on
  the session's current branch — #891 is on dev and is not its ancestor.
  `worktree.sh` bases new branches on `origin/dev` already, so this is automatic;
  it is recorded because a hand-made branch off the wrong HEAD would find no
  `ThemeTokens` to extend.

## Parallelität

**No cut. One strand.**

Reason: the five production edits form one dependency chain through a single
value. `tokens.rs` defines the constant, `theme_tokens.rs` selects it,
`theme.rs` emits it as a named colour, `compact_player_layouts.rs` consumes that
exact name, and the two moved tests straddle `compact_player_layouts.rs` and
`theme_tokens.rs`. There is no disjoint file group: any cut puts the token's
producer in one strand and its consumer in another, so the consumer's build
cannot go green before the merge *in principle* — the failure mode the
Parallelität section exists to prevent (Flathub wave, 2026-08-11, strand D).

The whole change is ~120 lines across five files in one crate. A cut would cost
two extra `cargo build`s of `reprise-gnome` to buy nothing.
