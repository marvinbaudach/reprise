---
slug: seek-playhead-16b
worktree: /home/marvin/Projects/reprise-seek-playhead-16b
branch: feature/seek-playhead-16b
phase: planned
codex_session:
created: 2026-08-05
---
# Spectral band (9b) + Playhead 16b

The seek bar gains two connected behaviours: every bar carries its dominant
frequency position as colour (variant 9b), and the playhead adopts the colour
of the position it covers, with an afterglow on the played side (variant 16b).

**The core specification that resolves every detail:** playhead colour and
afterglow depend on POSITION, never level. Level-coupled effects introduce a
second rhythm and make the playhead unusable as a position indicator. Existing
playhead code that reacts to level is replaced, not supplemented.

## Starting point in the code (`origin/dev`, `42ac117b46`)

- `crates/reprise-core/src/waveform.rs` — `WaveformAccumulator` folds decoded
  mono PCM into `STORED_PEAK_COUNT = 1000` amplitude bytes. `WaveformBackend`
  trait.
- `crates/reprise-platform-linux/src/waveform.rs` — a GStreamer pipeline decodes
  to 8 kHz mono F32 and pushes chunks through the accumulator.
- `crates/reprise-core/src/db.rs` — `tracks.waveform_peaks BLOB` column (schema
  v8), `SUPPORTED_SCHEMA_VERSION = 53`, `set_waveform_peaks` /
  `get_waveform_peaks` / `pending_waveform_tracks`.
- `crates/reprise-view/src/waveform.rs` — `shape_display_peaks`, percentile
  height mapping, `DisplayBar::{Silence, Level}`.
- `crates/reprise-gnome/src/ui/player_bar/waveform_seek.rs` — widget, `State`,
  frame-clock tick (`ensure_tick_callback`), gestures, `set_peaks`.
- `crates/reprise-gnome/src/ui/player_bar/waveform_seek_render.rs` — `draw`,
  `draw_bars`, `draw_playhead`. Colour currently comes from `area.color()`
  (`@reprise_player_accent`, derived from the cover).
- `crates/reprise-gnome/src/ui/playback/now_playing_wiring.rs:322` — loads cached
  peaks and calls `set_peaks` on the player bar and compact player.
- `data/brand/palette.toml` — `reprise_coral = "#FF6F5E"`,
  `reprise_teal = "#4FDBD4"`.
- `realfft = "3.5"` is already a `reprise-core` dependency.

## Decisions — do not renegotiate

1. **Fixed brand axis.** Bar colour comes from frequency position, no longer
   the cover: low position = `reprise_coral`, high position = `reprise_teal`.
   The cover accent remains on the Play button and other ink surfaces, but
   leaves the waveform. Decided by the user.
2. **Colour path through magenta, not grey.** A plain Oklab lerp from coral to
   teal passes through desaturated grey, while the mock-up shows a saturated
   magenta/violet middle. Interpolation therefore uses **Oklch with decreasing
   hue**: from about 30 degrees (coral), through magenta and blue, to about
   -170 degrees (teal), with linear lightness and chroma. This is the long path
   around the colour wheel and exactly the path shown by the mock-up.
3. **Absolute frequency scale, no per-track normalization.** Clamped `log2(Hz)`
   maps 120 Hz to 0.0 and 3000 Hz to 1.0. A bass-heavy track therefore stays
   warmer than a bright track, while colour can still move visibly within a
   track. The height scale remains percentile-normalized per track.
4. **Keep 8 kHz.** Do not change the extraction decode rate: peaks remain
   bit-identical, cached waveforms stay valid, and the 4 kHz Nyquist limit is
   enough for decision 3.
5. **Read playhead colour from stored support points, not display bars.**
   Otherwise colour would change with window width, contrary to checkpoint 5.
   The specification names `idx = fraction × 511` for 512 support points; here
   there are 1000, so use `idx = fraction × (n - 1)`. Do not hard-code 511.
6. **Remove the level-coupled playhead glow.** `playhead_glow_half_width` and
   `playhead_glow_alpha` currently follow `bass_pressure`; the specification
   forbids precisely that at the playhead. The new glow is position-coloured
   and constant-width. `played_light` / `played_swell` on the **bars** remain
   untouched; they are outside this specification.
7. **No glow or shimmer in the mini player** (`fill_bars`). They would wash out
   the 46-bar strip; this is the established rule. The mini player still gets
   spectral bar colours.

---

## Package A — Derive the frequency position (`reprise-core`)

Reprise already computes and stores a spectrogram per track (24 logarithmic
bands, 20 Hz–16 kHz, 20 frames/s, from 32 kHz PCM; see
`docs/research/spectrogram-pipeline.md`). The colour curve is derived from it.

**This replaces the plan's original packages A and B**, which measured a second
spectral centroid from an 8 kHz decode and stored it in a column of its own.
That was written before the spectrogram pipeline landed on `dev`. Keeping both
would mean two decodes, two backfills, two migrations and two answers to the
same question — and the 8 kHz path stopped at 4 kHz, so cymbals and hi-hats,
the very thing that makes a track sound bright, never reached the measurement.

- `TrackSpectrogram::centroid_curve(buckets)` in `crates/reprise-core/src/spectrogram.rs`:
  each stored cell back to a linear amplitude, the energy-weighted mean of the
  band centres in octaves per bucket, then the per-track percentile window from
  decision 3. Silent buckets inherit the last valid colour.
- `waveform_cache::centroid_for_playback` reads the stored spectrogram beside
  the peaks. No new column, no new migration, no second decode.
- A track without a stored spectrogram yields `None`, and the bar draws in the
  plain accent exactly as it did before there was a spectral axis.

## Package C — Colour function (`crates/reprise-view/src/spectral_colour.rs`, new)

Keep it toolkit-neutral and pure so it can be tested without a display.

- `pub const CORAL: (u8,u8,u8)` / `pub const TEAL: (u8,u8,u8)` come from
  `data/brand/palette.toml`.
- `pub fn spectral_colour(t: f64) -> (f64, f64, f64)` clamps `t` to 0..1,
  interpolates in Oklch according to decision 2, and returns sRGB in 0..1.
- `pub fn shape_centroid(raw: &[u8], count: usize) -> Vec<f32>` averages into
  display bars, analogous to `aggregate_rms`, and returns 0..1.
- `pub fn centroid_at(raw: &[u8], fraction: f64) -> f64` uses **linear
  interpolation between adjacent support points** at
  `idx = fraction × (n - 1)`. Do not select the nearest point, which would make
  the colour visibly jump between buckets. An empty curve returns 0.5.
- `pub fn smooth_towards(current: (f64,f64,f64), target: (f64,f64,f64), dt_s: f64, tau_s: f64) -> (f64,f64,f64)`
  performs component-wise exponential smoothing. The exact formula deviation
  needed to reconcile the prose and test is recorded in `.pipeline-codex.md`.
  Use `tau = 0.120`.

Reuse the existing Oklab/Oklch code from
`crates/reprise-gnome/src/ui/style/cover_accent_oklab.rs`. If it must move into
`reprise-view`, move it rather than writing a second conversion. There may be
only one conversion implementation.

## Package D — Draw bars and playhead (`waveform_seek_render.rs`)

### D1. Bar colour

`draw_bars` colours every played bar with
`spectral_colour(shaped_centroid[index])` instead of the accent. Unplayed bars
remain white at `UNPLAYED_ALPHA`; hover preview and ghost remain unchanged.
`scale_chroma` / `desaturation_progress` still affect bar colour to preserve
pause desaturation. With no centroid curve (an old track awaiting analysis),
keep today's accent path unchanged: never show empty or grey bars.

### D2. Playhead geometry

Use a 3 px wide rounded bar (radius 1.5) extending 3 px above and below
`max_bar_height`. `rounded_bar` in `waveform_primitives.rs` already draws it.

### D3. Glow

Draw the same rounded shape three times, each 2 px wider and taller, at alpha
`0.35 / 0.18 / 0.08` in the smoothed playhead colour. Draw from outside
(weakest) inward, followed by the playhead. Use normal `OVER`, no
`Operator::Add`, no gradient. Remove the old pressure-driven gradient glow,
`playhead_glow_half_width`, `playhead_glow_alpha`, and their tests.

### D4. Afterglow

Immediately left of the playhead, draw a 14 px rectangle at full
`max_bar_height` with a horizontal gradient from alpha 0 at `head - 14` to
0.33 at `head`, in the smoothed playhead colour and with `OVER` over the bars,
so they brighten instead of being replaced. Clip the left edge to `x >= 0`.
The direction is fixed: the shimmer trails; it never runs ahead.

The 14 px width matches the bar spacing (3 px bar, 2 px gap: almost three
bars) and is an **absolute pixel measure**; it does not scale with widget
width. The same holds for playhead width and overhang (checkpoint 5).

### D5. Colour tracking in widget state (`waveform_seek.rs`)

- `head_colour_target` is
  `spectral_colour(centroid_at(raw_centroid, fraction))`.
- `head_colour` is the smoothed result, advanced in the tick by
  `smooth_towards` with real frame `dt`, not in draw, whose cadence is uneven.
- Without a centroid curve, playhead colour remains the accent as before.

## Package E — Drawing and cadence

### E1. Surface cache

GTK4 no longer supports partial invalidation; the gain comes from not
tessellating bars every frame. Keep two `cairo::ImageSurface` values in
`State`:

- `mask_surface` (`Format::A8`): the pure bar silhouette at alpha 1.0.
- `colour_surface` (`Format::ARgb32`): the same bars in their spectral colour.

Per frame, only blit these layers:

| Layer | Source | Clip |
|---|---|---|
| unplayed | white at `UNPLAYED_ALPHA`, `mask_surface` | `x >= head` |
| hover preview | white at `HOVER_PREVIEW_ALPHA`, `mask_surface` | `head ... hover` |
| ghost | accent at `GHOST_ALPHA`, `mask_surface` | ghost area |
| played | `colour_surface` | `x <= head`, masked by a linear gradient that reproduces today's `played_alpha` (`PLAYED_MIN_ALPHA` at track start to 1.0 at the playhead) multiplied by `played_light(pressure, swell)` |

`played_alpha` is linear by bar index, so a gradient reproduces it exactly.

Rebuild only on track change (`set_peaks`), width, height or scale-factor
change, changed bar count, changed widget colour (theme switch: compare
`area.color()` with the last built value), or changed
`desaturation_progress`.

Use the cache only when settled (`build_progress >= 1.0 &&
crossfade_progress >= 1.0` and no desaturation animation is running). During
build, crossfade and desaturation, retain the current direct drawing path;
these are short, rare states in which every frame differs anyway.

### E2. Redraw brake

Continue using `add_tick_callback` on the frame clock, never `glib::timeout`.
At normal width the playhead moves about two pixels a second, so 60 redraws a
second add no value.

Call `queue_draw()` from the tick only when one of these conditions holds:

- playhead position moved at least **1 px** since the last drawn frame;
- smoothed colour changed measurably (threshold 1/512 per channel);
- build, crossfade, or desaturation is running; or
- hover, drag, or level state changed.

Put this decision in a pure
`should_redraw(...) -> bool` function in `waveform_primitives.rs` so it can be
tested without a display. Set `last_drawn_head_x` / `last_drawn_colour` **in
`draw`**, not the tick; that is the truth about what reached the screen.

The `settled` condition that stops the tick must additionally require smoothed
colour to have reached its target, or colour freezes halfway when position is
stationary.

## Package F — Respect user state

- With `gtk-enable-animations = false` (`motion::animations_enabled()`), omit
  shimmer and glow completely. Draw the playhead as a solid shape in the
  position colour without temporal smoothing, because smoothing itself is an
  animation. Both effects are decoration under this setting.
- During dragging (`drag_fraction.is_some()`), colour coupling continues but
  **shimmer is absent**. Scrubbing jumps across positions, so an afterglow at a
  point that was never played would be false.

---

## Tests

Everything testable without a display belongs in pure functions. The display
suite is unreliable in the group and cannot serve as proof.

### `reprise-core`

- `SpectralAccumulator`: a 200 Hz sine produces substantially lower bytes than
  a 3 kHz sine; a low-to-high sweep produces a monotonically increasing curve.
- Output length equals `buckets`, independent of `push` chunk size (the same
  sine in chunks of 1, 100 and 10000 produces the same result).
- Silent buckets inherit the last valid value; a fully silent stream is 128
  throughout.
- Amplitude peaks remain unchanged (regression test against existing expected
  values).

### `reprise-view`

- `spectral_colour(0.0)` approximates coral and `spectral_colour(1.0)`
  approximates teal within 1/255.
- The path between them stays saturated: at `t = 0.5`, chroma exceeds a minimum
  threshold. This fails if someone regresses to an Oklab lerp.
- `centroid_at` interpolates linearly: halfway between two support points it
  returns their mean, not either endpoint.
- `centroid_at` handles edges (0.0 and 1.0), length 1, and an empty curve.
- `smooth_towards` covers about 63 percent of the distance when `dt == tau`,
  does not move for `dt == 0`, and converges monotonically.

### `reprise-gnome` (pure functions, no display)

- `should_redraw`: 0.9 px movement with unchanged colour does not redraw;
  1.1 px does; unchanged position with changed colour redraws.
- Playhead geometry: 3 px wide, height `max_bar_height + 6`, centred on
  `head_x`.
- Glow layers: three, widths `+2/+4/+6`, alphas `0.35/0.18/0.08`.
- Shimmer range: `(head - 14, head)`, clipped to 0 on the left; empty while
  dragging and when animations are disabled.
- Shimmer lies left of the playhead; a test pins the direction.

### Database

- Migration v54 from a v53 database; round-trip peaks and centroid; mismatched
  centroid length reads as `None`; `pending_waveform_tracks` returns a track
  with peaks but no centroid.

## Manual verification (after building)

- Compare a bass-heavy and a bright track: the playhead must visibly move from
  coral to teal over the length. If it stays one colour, frequency mapping is
  not active.
- Compare CPU load during playback with and without the redraw brake.
- Resize the window to 3000 px: shimmer width and playhead stay constant; both
  are absolute pixels, not proportions.

## Non-goals

- No change to height mapping, build-up, crossfade, or bar `played_light` /
  `played_alpha`.
- No change to sample rate or peak calculation.
- No library-wide backfill pass; existing background analysis supplements the
  frequency position.
- No glow or shimmer in the mini player.
