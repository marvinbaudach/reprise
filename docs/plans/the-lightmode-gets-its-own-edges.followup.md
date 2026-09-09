# Follow-up to `the-lightmode-gets-its-own-edges`

Raised by the user against a real light-mode screenshot **while the first Codex
run was still going**. Not folded into the running plan on purpose — this is a
second round in the same worktree.

Same hard rule as the parent plan: **the dark appearance must not change.** The
parent plan's mechanism (a `@define-color` in `theme_css` whose dark branch
reproduces today's value) does not reach the items below, because both live in
Rust, not CSS. They need an explicit `libadwaita::StyleManager::default()
.is_dark()` branch, and the dark arm must return today's constant unchanged.

---

## A — the cover bloom is illegible in light

> „kannst du im lightmode auch diesen effekt um das cover stärker machen? man
> sieht es im light sehr undeutlich. wirkt eher wie nen fehler"

There are two separate layers around the Now Playing cover, and only one of them
is the one in the screenshot:

| Layer | What it is | Where |
| --- | --- | --- |
| Accent glow | a teal `radial-gradient` behind the cover, `alpha(@reprise_player_accent, 0.15)` | `now_playing/surface_css.rs:14-18`, `.reprise-now-playing-glow` |
| **Cover bloom** | the **blurred album artwork** itself, painted as a texture and breathed at a per-frame opacity | `now_playing/cover_bloom.rs` → `cover_bloom_area.rs::set_light` |

The halo visible in the screenshot carries the artwork's own browns and oranges,
not mint — so it is the **cover bloom**, not the accent glow.

Its opacity model, `cover_bloom.rs:39-41`:

```rust
const REST_OPACITY: f64 = 0.06;
const OPACITY_PER_PRESSURE: f64 = 0.15;
const OPACITY_PER_SWELL: f64 = 0.16;
```

so it breathes between 0.06 and about 0.37. Over a near-black panel that is a
glow. Over `#eceef1` the same blurred artwork at 6–37 % is a low-contrast grey-
brown smear with no edge — which is exactly why it reads as a rendering fault
rather than as an effect.

### What to change

Add a light triple next to the dark one, chosen by `is_dark()` at the point
where the three constants are read (`cover_bloom.rs`, in or just above
`set_light`). Do **not** change the existing three values — they are the dark
arm and must stay literally as they are.

```
light: REST 0.14   PER_PRESSURE 0.26   PER_SWELL 0.24
```

That roughly doubles the rest state and lifts the peak to about 0.64. The
breathing *ratio* stays close to the dark one, so the motion still reads as the
same effect rather than a different one.

Why raising alpha is the right lever and not the only one: on a dark ground the
bloom gains contrast by being *lighter* than the panel, and a blurred cover is
almost always lighter than near-black. On a light ground it has to be *darker*
than the panel to be seen at all, and a blurred cover is usually mid-tone, so
the same alpha buys far less separation. If after the change it still reads as
grubby rather than deliberate, the next lever is saturation (paint the blurred
texture through a saturating filter in light) — **not** more alpha. Do not add
that speculatively; land the alpha change first and let the user look.

### Also reconsider the accent glow's light value

The parent plan's step 6 sets `reprise_now_playing_glow` to
`alpha(@reprise_player_accent, 0.05)` in light, down from 0.15. Keep that. The
two layers are doing different jobs: a broad mint wash over white is what makes
the panel look dirty, while the artwork bloom is what should carry the halo.
Weaker teal *and* stronger artwork is the coherent pair, not a contradiction.

### Verification

- `cover_bloom.rs`'s existing tests must stay green.
- Add a test asserting the dark arm still returns exactly `0.06 / 0.15 / 0.16`,
  written as literals, so a later retune of the light arm cannot drift dark.
- Assert the light arm's rest opacity is greater than the dark arm's — the
  regression fence for "someone unified them again".

---

## B — accent text is unreadable on light surfaces

> „die accentfarbe ist auch schlecht zu lesen"

Measured, not eyeballed. `scripts/measure-contrast.py` over the user's light-mode
screenshot, one region per accent-coloured element:

| Region | Surface | Glyph | Ratio | |
| --- | --- | --- | --- | --- |
| Rating stars (track table) | `#f9fbfc` | `#006965` | **6.31** | ok |
| Filter-chip label | `#ddf1f2` | `#006461` | **5.99** | ok |
| Now-playing row title | `#e8f7f7` | `#006460` | **6.38** | ok |
| `62` in `62 of 1,969 tracks` | `#f4f5f7` | `#3e9886` | **3.19** | fail |
| Search-match "Chelsea" | `#dcf5f5` | `#6ae0dc` | **1.38** | fail |
| Play-button glyph | `#50dcd4` | `#ffffff` | **1.68** | fail (PLAY-16) |

So the derived role already works where it is used: everything painted with
`@reprise_accent_text_color` comes out as a dark teal around `#006463` and
clears AA comfortably. The picture reads as "all the mint is unreadable" because
the three failures sit in the busiest parts of the view — but only three sites
are actually broken, and they break for three different reasons.

**1. The search-match highlight — the worst one, and the one the parent plan
does not touch.** `crates/reprise-gnome/src/ui/search_highlight.rs:94`
`accent_palette()` builds its Pango attributes in Rust from
`style::accent::accent_rgba()` — the **raw** accent — for both the foreground
and an 18 % background wash. Nothing in CSS can reach it, so step 3 leaves it at
1.38:1.

Route it through the same derivation `theme_css` uses:
`accent::accent_text_color(accent, palette.critical_accent_surface(is_dark,
accent), is_dark)`. Do not write a second formula — markup and CSS must not be
able to disagree. Note the highlight paints its own 18 % accent background, so
the ratio that has to clear 4.5:1 is the derived foreground against *that*
composite, not against the plain view background; assert exactly that pair.

**2. The result count.** `filter_bar_strings.rs:67` wraps the value in `<b>` and
the label carries GTK's `.accent` class, which libadwaita paints with
`@accent_color`. **The parent plan's step 3 fixes this** — light `accent_color`
becomes the derived teal. No extra work; just confirm it measures ≥ 4.5:1
afterwards.

**3. The play glyph.** White on the accent fill, the documented `PLAY-16`
exemption. Step 3 also improves this in light, because
`@define-color reprise_player_accent @accent_color` makes the fill follow the
darkened teal. Re-measure after step 3 and update the `PLAY-16` comment with the
light figure. Do not remove the exemption.

Everything else the sweep found already reads `@reprise_accent_text_color` or
`@reprise_player_accent` and needs nothing.

### Non-negotiable

`AccentSource::System` must keep getting no app-authored accent role
definitions, in both appearances. libadwaita owns those names there and derives
its own contrast-safe `accent_color`.

---

## C — the seek waveform is washed out in light

> „und der seek ist sehr schwach im lightmode. gern deutlich stärker werden"

This **reverses** the parent task's "do not touch the spectrum seek" line. That
exclusion was written because the seek is a feature rather than a styling bug;
the user has since asked for it explicitly, for the light appearance only.

`crates/reprise-gnome/src/ui/player_bar/waveform_seek.rs` sets the whole colour
model, and **nothing in the player-bar seek code reads `is_dark`** — the
waveform is appearance-blind today. The constants were tuned against the dark
panel, and `waveform_seek.rs:47-51` says so in as many words: *"Measured, not
chosen: below this the deep-blue stretches of a bass intro disappear against the
bar's own background."* That measurement was taken on a dark ground and does not
transfer.

| Constant | `waveform_seek.rs` | Dark (keep) | Light |
| --- | --- | --- | --- |
| `UNPLAYED_ALPHA` | :52 | `0.34` | `0.55` |
| `HOVER_PREVIEW_ALPHA` | :54 | `0.62` | `0.78` |
| `BUFFERED_ALPHA` | :78 | `0.48` | `0.66` |
| `SECTION_MARK_ALPHA` | :70 | `0.30` | `0.42` |
| `GHOST_ALPHA` | :85 | `0.40` | `0.55` |
| `PLAYHEAD_ALPHA` | :80 | `0.70` | `0.85` |

Read them through one appearance-aware accessor rather than branching at each
use site, so a later retune cannot update half of them.

### Two things alpha alone will not fix

1. **A white literal in the fallback/mini renderer.**
   `waveform_seek_render.rs:367` paints unplayed bars as
   `cr.set_source_rgba(1.0, 1.0, 1.0, UNPLAYED_ALPHA * 0.6)` — white at 20 %,
   which is *invisible* on a light panel, not merely weak. It must become the
   appearance's foreground: white in dark exactly as today, and the window
   foreground colour in light. This is the single worst offender in the module.

2. **Spectral bars are pastel.** In `SeekColouring::Frequency` the bar colour
   comes from the audio, and the high-frequency end lands on pale pinks and
   light blues. Those have *higher* luminance than the light panel, so no amount
   of alpha makes them read — raising alpha only makes a pale bar paler-looking
   against white. In the light arm, clamp each bar colour's lightness so it
   stays a minimum distance below `@view_bg_color`'s luminance, keeping hue and
   chroma. `crates/reprise-gnome/src/ui/style/color_math.rs` already has the
   OKLab lightness machinery (`linear_rgb_to_oklab`, `oklab_to_linear_rgb`,
   `relative_luminance`) — reuse it, do not write a second colour model.

   Do this **after** the alpha change and check it separately: if the alphas
   alone already read well, the clamp is not needed and should not be added.

### Verification

- `waveform_seek_colour_tests.rs` and `waveform_seek_tests.rs` stay green.
- Add a test pinning every dark value in the table above to its literal, so the
  light arm cannot drift dark.
- Add a test that no unplayed-bar path paints pure white when the appearance is
  light.

---

## D — undo one over-correction from the review round

`crates/reprise-gnome/src/ui/style/buttons.rs:223-224`.

The review round demanded that `.reprise-panel-toggle.reprise-sidebar-toggle:checked:active`
keep the value it had on `origin/dev`, `alpha(@accent_bg_color, {HOVER_BG_ALPHA})`,
on the grounds that the dark appearance must not move. **That reasoning was
wrong and the change must be reverted.**

The sidebar toggle's de-colouring was explicitly approved for *both*
appearances — it is the one deliberate exception to the dark-identity rule in
this whole branch. Its `:checked` state is now `transparent` and its
`:checked:hover` is `alpha(currentColor, {BTN_HOVER_ALPHA})`. Leaving the
pressed state on an accent fill makes it the only state still showing the
accent, so a button whose checked look was deliberately neutralised flashes the
accent when pressed. That is incoherent.

Change it to the flat-button press value, matching the family the other two
states now belong to:

```
.reprise-panel-toggle.{sidebar_toggle}:checked:active {
  background-color: alpha(currentColor, {BTN_PRESS_ALPHA}); }
```

Keep the selector and its specificity exactly as they are — the cascade fix from
the review round is correct and must not be touched.

The test written in that round asserts the `:checked:active` declaration
references `{HOVER_BG_ALPHA}`. Update that assertion to `{BTN_PRESS_ALPHA}` and
reword whatever the test name or comment claims, so it states the intent that
actually holds: the sidebar toggle carries no accent in any of its checked
states.
