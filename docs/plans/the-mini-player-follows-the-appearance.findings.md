# Findings — the mini player follows the appearance

**Date:** 2026-09-09
**Reported as:** "die miniview sollte auch im gleichen style und mode erscheinen
wie der große player" (light-mode screenshot of the mini player)
**Base for the work:** `origin/dev` @ `0970978e70`. The dedicated feature
worktree was created from that exact revision, which includes `abb205cef9`
("The light appearance gets its own edges", #891) and its
`style/theme_tokens.rs` appearance channel.

---

## Measured, not eyeballed

Source pixels: `~/.claude/image-cache/71755909-…/1.png` (908×200, mini window on
a black backdrop). Sampled with PIL; ratios are WCAG relative luminance.

| Region | Colour | Against | Ratio | |
| --- | --- | --- | --- | --- |
| Card surface | `#222222` | — | — | = `rgba(34, 34, 34, 0.92)` over black |
| Title glyph (`Pretty Lies`) | `#1a1c1f` | card | **1.07** | invisible |
| Artist glyph (`If Not for Me`) | `#1d1e20` | card | **1.05** | invisible |
| Play-button fill | `#006f6c` | — | — | light-derived accent |
| Play glyph (white) | `#ffffff` | fill | 6.02 | ok |

Two of these settle the diagnosis on their own:

- `#1a1c1f` **is** `Theme::PerpetualRain.light_palette().fg`, byte for byte.
- 6.02:1 is the exact figure `player_bar_layout.rs` records for the light
  appearance ("in light, the playback accent … raises the ratio to 6.02:1").

So the mini window **does** receive `theme_css` and **is** in the light
appearance. Everything in the card follows the light theme except one literal.

The bug is therefore not cosmetic: **in light mode the mini player's title and
artist are unreadable at ~1.05–1.07:1**, not merely "on the wrong grey".

The row profile is worth keeping: at a `Σ|Δ| > 25` threshold nothing at all
shows above the waveform — the text only appears at `> 4`. A sweep that looks
for "where is the text" with a normal threshold concludes there is none.

## Cause

`crates/reprise-gnome/src/ui/compact/compact_player_layouts.rs:164` `mini_css()`:

```
.mini-player-card { background-color: rgba(34, 34, 34, 0.92);
                    border: 1px solid alpha(white, 0.09); }
.mini-player-cover { box-shadow: inset 0 0 0 1px alpha(white, 0.08); }
.mini-player-artist { color: alpha(@window_fg_color, 0.6); }
```

Three dark-only literals; the foreground next to them is a theme token. #891
converted the whole GTK frontend to appearance-aware tokens and **skipped this
file** — `compact_player_layouts.rs` is not in that commit's file list.

`mini_css()` is joined into `app_css()` (`style/mod.rs:147`), which is built once
and never learns `is_dark`. Per the #891 handover that is not a preference: a
`@define-color` emitted by `theme_css` is the only channel. A Rust-side
`is_dark` branch here would not work.

## Proposal — four tokens, dark branches byte-identical

Emitted by `theme_css` via `ThemeTokens::for_appearance`, dark arm reproducing
today's literal so dark pixels are provably unmoved (the property #891 is built
on, proven by `dark_appearance_tokens_reproduce_every_replaced_literal`).

| Token | dark (unchanged) | light |
| --- | --- | --- |
| `reprise_mini_card_bg` | `rgba(34, 34, 34, 0.92)` | `alpha(@headerbar_bg_color, 0.92)` |
| `reprise_mini_card_edge` | `alpha(white, 0.09)` | `rgba(0, 0, 6, 0.14)` (= `PILL_BORDER_LIGHT`) |
| `reprise_mini_cover_edge` | `alpha(white, 0.08)` | `alpha(#000000, 0.10)` |
| `reprise_mini_artist_fg` | `alpha(@window_fg_color, 0.6)` | `alpha(@window_fg_color, 0.70)` |

The artist token has to be a whole colour, not a bare alpha — a GTK named
colour holds a colour, and every other `ThemeTokens` field is a complete CSS
value. That also keeps its dark-identity entry the same shape as the other
sixteen in the `definitions` array.

`alpha()` over a named colour inside `@define-color` is already proven in
production: dev ships `@define-color reprise_hover_bg alpha(@accent_bg_color,
0.10);`, and `theme_css` defines `@headerbar_bg_color` earlier in the same
string. Still cheap to assert once via `style::mod::css_parse_errors()` on the
`is_dark = false` output — this file's scar is a fix that "shipped doing
nothing" because GTK4 silently dropped declarations it could not parse.

No existing token can be reused: `HAIRLINE_DARK` is `0.06` and
`COVER_EDGE_DARK_ALPHA` is `0.12` — neither matches the mini card's `0.09` /
`0.08`, and adopting a near-miss would move dark pixels.

### Why the artist alpha needs a light branch

The card is glass (0.92) on a transparent toplevel, so its composite depends on
the desktop behind it. Worst case over the three light palettes × three
backdrops (white / mid-grey / black), artist = `alpha(fg, a)` over the card:

| light card surface | a = 0.60 | a = 0.65 | a = 0.70 |
| --- | --- | --- | --- |
| `@headerbar_bg_color` | 3.95 – 4.24 | 4.55 – 4.95 | **5.27 – 5.80** |
| `@sidebar_bg_color` | 4.00 – 4.29 | 4.62 – 5.02 | 5.36 – 5.89 |
| `@card_bg_color` | 4.23 – 4.50 | 4.93 – 5.30 | 5.78 – 6.29 |

**No surface clears 4.5:1 at today's 0.60.** 0.70 clears everywhere. The title
(full `@window_fg_color`) lands at 14.4:1 on `@headerbar_bg_color`.

`@headerbar_bg_color` is what the big player bar paints
(`player_bar_layout.rs`, `.player-bar-surface`) — the literal reading of "same
style as the big player", and the recommendation.

Worth one line of doubt: it is also the *worst* row of that table, and the mini
card is a floating rounded card on a transparent toplevel rather than a docked
bar. `@card_bg_color` is the alternative with ~0.5 more headroom, and this
codebase already re-picks light surfaces by role instead of mirroring the dark
one (`PILL_BG_DARK = @sidebar_bg_color`, `PILL_BG_LIGHT = @card_bg_color`).

## Tests that move

- `compact_player_layouts.rs:242` `mini_1_card_css_matches_frame` asserts the
  literal `rgba(34, 34, 34, 0.92)` lives in `mini_css()`. After tokenising it
  will not. **Relocate**, do not delete: the dark-identity assertion belongs in
  the `definitions` list of `theme_tokens.rs`'s
  `dark_appearance_tokens_reproduce_every_replaced_literal`, as #891 did for
  every other token.
- `compact_player_layouts.rs:~350` `mini_artist_contrast_on_tint` hardcodes
  `bg = 34.0/255.0` and `ARTIST_ALPHA = 0.6`. Needs both arms, and the light arm
  must sweep the three light palettes × the backdrop range — that test is what
  picks the alpha, so it comes before the value.
- `panel_contrast.rs:255` already consumes `mini_css()` twice
  (`PANEL_ROLES` role check, `play_16_…`). Re-check `.mini-player-card` still
  satisfies its `PANEL_ROLES` role once the background is a token.
- `compact_player_tests.rs:201,240` also read `mini_css()`.

## Not in scope

Android's mini player (`LibraryFrame.kt:255`) already uses
`MaterialTheme.colorScheme.surfaceContainer` and follows the appearance. This
is desktop-GTK only.
