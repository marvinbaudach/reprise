# The sheen goes; a Kantenlicht and a Schimmer take its place

> **For agentic workers:** implement task by task, test-first, one commit per
> task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Replace the travelling reflection across the cover with the two light
treatments the design mockup actually specifies — a one-pixel **Kantenlicht**
hugging the cover's contour, and a **Schimmer**: a slow conic sweep of the
cover's three dominant colours behind it, one turn a minute.

**Why:** the owner looked at the sheen in use and rejected it — a band crossing
the artwork reads as a smear over the picture. The mockup answers the same
question twice, and both answers are better: light *on the edge* of an object,
and light *turning behind* it.

**What stays:** the blurred-cover backdrop (`cover_bloom.rs`) is explicitly
kept — the owner likes it. The mockup's own backdrop variant (three drifting
radial blobs) is **not** adopted. The Schimmer is layered on top of the existing
bloom, a composition the mockup does not show but the owner asked for; see the
note in Task 6 about judging the combined weight.

**Every number in this plan is copied from the mockup**
(`Musik-reaktive Cover-Effekte.dc.html`, project
`048d9cd6-e6f0-4d5a-8c7a-50563d554ef7`). Where the mockup's CSS has no direct
GTK equivalent the adaptation is spelled out and justified inline. Do not
invent values, and do not "improve" the ones given.

---

## What already exists — read before writing anything

- **`ui/cover_lift.rs`** (548 lines) owns the cover treatment: two cached shadow
  layers whose opacities cross-fade, the sheen, and
  `CoverLiftSource::{Swell, Kick}` with a 400 ms Ambient cross-fade between the
  two readings. `CoverLift::new(cover, width)` wraps the cover in an
  `Overlay`; only the 168 px panel cover gets the sheen, the 56 px bar cover
  does not.
- **`ui/now_playing/cover_bloom.rs`** (314 lines) is the blurred-cover backdrop:
  a `DrawingArea` in the panel's `head_overlay`, drawing a 330 px band at 124 %
  panel width. It owns **the frame clock for the whole reactive-light layer**:
  while playing, the spectrum events drive it (every 11.6 ms); while paused, a
  33 ms tick callback does; when pinned (Visualizer tab open, plugin off, panel
  hidden) it emits `on_frame(0)` and stops. Everything else hangs off its
  `set_on_frame`.
- **`ui/now_playing/now_playing.rs:543 advance_swell`** is the single fan-out
  point: it holds `pressure` and computes `swell`, then feeds
  `cover_lift`, `bloom` and `visualizer`. Every new consumer wires in here.
- **`ui/style/cover_accent.rs`** (652 lines) extracts **one** colour from the
  cover: `accent_from_cover_file` → `Option<Rgb>` → a `@define-color
  reprise_player_accent` override in a dedicated high-priority `CssProvider`,
  cross-faded over the Ambient token. Internally it *already* median-cuts the
  cover into **8 buckets** and scores them by `population × OKLCH chroma` — it
  then throws seven away. The three-colour palette is mostly a matter of not
  discarding them.
- Only three call sites touch that API: `playback/now_playing_wiring.rs:59`
  (extract), `:68` and `:231` (cross-fade), and `updates/release_cover.rs:23`
  (release notes — wants one colour).
- The panel cover already carries a static rim,
  `inset 0 0 0 1px rgba(255,255,255,.12)`. That stays: it is the object's own
  contour, and the mockup has it too, on the same covers that get the
  Kantenlicht.

### The mockup's palette

```
--l1: 145 132 217;   --l2: 120 140 210;   --l3: 170 125 190;
```

and its own description of where they come from:

> Drei Farben, nach Sättigung gewichtet und auf gleiche Helligkeit gebracht.
> Near-Black fällt raus — daraus kommt kein Licht.

That is exactly what `oklch_clamp` already does for one colour (L into
[0.55, 0.75], C into [0.08, 0.13], reject C < 0.03). Task 4 applies it to three.

---

## Global constraints

- **Worktree:** `/home/marvin/Projects/reprise-reactive-light`, branch
  `feature/reactive-light`. Do not touch files outside it. Do not push.
- `cargo test -p reprise-gnome --bins <filter>` — it is a binary crate.
- **Do not run display tests and do not start Xvfb.** Write them, leave them
  `#[ignore = "requires a display; run via xvfb-run"]`, name them in your
  summary, continue. (For the record, the reviewer runs them as
  `env -u WAYLAND_DISPLAY GDK_BACKEND=x11 xvfb-run -a …` — plain `xvfb-run`
  does not isolate a GTK4 app on a Wayland session and puts test windows on the
  owner's desktop.)
- `export XDG_CACHE_HOME="$PWD/.cache-test"` before any test run.
- **Known-red on this base — not yours, do not chase:**
  `browse_bar::…widget_projects_removable_chips_without_a_redundant_reset_button`,
  `song_visualizer::…ac_23_the_readout_follows_the_measurement_the_player_delivers`,
  `song_visualizer::…bars_fullscreen_render_budget_diagnostic`,
  `stats_view::…stats_19_period_switch_tweens_bars_without_restarting_static_content`,
  `stats_view::…stats_11_realistic_width_keeps_the_hero_copy_unellipsized`.
- **Do not touch:** the shadow lift's cross-fade maths, the bloom's own drawing
  and cache, the playhead glow, the marker tempo, the scroll glide,
  `bass_pressure.rs`, `shape_display_peaks`, `eq_bars.rs`, the play button, the
  seek bar.
- **File cap is 800 lines and three files are already close.** Each task that
  would breach it carries its split as an explicit step. Do not skip the split
  and do not invent a different one.
- One commit per task, no attribution footer.

---

## Task 1: The sheen goes

**Files:**
- Modify: `crates/reprise-gnome/src/ui/cover_lift.rs`
- Modify: `docs/ux-rules.md` (AC-24)

- [ ] **Step 1: Amend AC-24 first — docs beat code**

Find the AC-24 clause describing the cover's "travelling reflection" and replace
that clause with:

```markdown
  it lifts on its shadow, carries a one-pixel light seam along its edge, and
  has a slow conic sweep of the cover's own palette turning behind it — one
  turn a minute. The seam sits one pixel outside the artwork, so the cover's
  footprint grows by exactly one pixel on each side; nothing crosses the
  picture itself.
```

- [ ] **Step 2: Delete the sheen**

Remove from `cover_lift.rs`: the `SHEEN_*` constants, `SHEEN_CLASS`,
`sheen_opacity`, `sheen_offset`, `CoverSheen`, `draw_sheen`, the
`rounded_rectangle` helper (it exists only for the sheen), the `sheen` field of
`CoverLiftWidgets`, its construction in `CoverLift::new`, and its line in
`apply_reading_to_widgets`. Drop the `.{SHEEN_CLASS}` rule from `css()`.

`set_frame_time` currently exists only to drive the sheen. **Keep the method and
keep its `motion::animations_enabled()` gate** — Task 5 needs exactly this clock
for the Schimmer's rotation. For this task it may store the value and do nothing
else.

Delete the test `ac_24_the_sheen_travels_on_time_and_only_brightens_on_the_swell`
rather than leaving it asserting a thing that no longer exists. **Keep** its last
two lines' idea by moving this assertion into the test you write in Task 2:

```rust
        // Live spectrum frames and the backdrop's paused breath are the only
        // frame sources; the cover must not own another timer.
        let timer_api = ["add_tick", "callback"].concat();
        assert!(!include_str!("cover_lift.rs").contains(&timer_api));
```

- [ ] **Step 3: Verify and commit**

```bash
export XDG_CACHE_HOME="$PWD/.cache-test"
cargo test --workspace
bash scripts/check-ux-traceability.sh
git add -A
git commit -m "refactor(now-playing): drop the travelling sheen"
```

---

## Task 2: The Kantenlicht

**Files:**
- Modify: `crates/reprise-gnome/src/ui/cover_lift.rs`
- Modify: `crates/reprise-gnome/src/ui/now_playing/now_playing.rs` (`advance_swell`)

**Interfaces:**
- Produces: `edge_opacity(pressure: f64, swell: f64) -> f64`;
  `CoverLift::feed(&self, swell: f64, kick: f64, pressure: f64)`.

### The design

> **Kantenlicht** — "Eine Kontur, die Licht fängt. Ein Pixel breit, nichts
> dahinter."

```css
position: absolute; inset: -1px;          /* one pixel OUTSIDE the cover */
border-radius: 9px;                       /* the cover's radius + 1 */
border: 1px solid rgb(var(--l1));
opacity: calc(0.18 + var(--pres) * 0.10 + var(--sw) * 0.22);
```

**Adaptations, and why:**

- `inset: -1px` is an *outset* rim. GTK4 has no negative margins, so the rim is
  a sibling layer in the existing overlay, sized `width + 2` and centred. The
  assembly's footprint therefore grows by 1 px on each side. That is what the
  design asks for; do not "fix" it back to inset.
- The mockup's `border-radius: 9px` belongs to a card whose cover uses an 8 px
  radius. Ours is `RADIUS_SURFACE` = `12px`, so the rim's radius is **13px** —
  the same "+1" relation.
- `rgb(var(--l1))` is the primary palette colour, which is exactly today's
  `@reprise_player_accent`. Use that; Task 4 does not change it.
- The rim uses `pressure` **and** `swell` — not the `Source` switch. The switch
  exists so the *shadow* answers the beat inside the Visualizer view; the design
  gives the rim one formula with no such distinction. Leave the switch alone and
  do not route the rim through it.

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn ac_24_the_edge_light_rides_a_pressure_bed_under_a_swell() {
        // Straight from the mockup: 0.18 + 0.10·pres + 0.22·sw.
        assert!((edge_opacity(0.0, 0.0) - 0.18).abs() < 1e-9);
        // A held breakdown: no swell left, but the contour stays lit.
        assert!((edge_opacity(1.0, 0.0) - 0.28).abs() < 1e-9);
        // A broad swell on a lit bed.
        assert!((edge_opacity(0.85, 0.8) - 0.441).abs() < 1e-9);
        // Both at full: the ceiling.
        assert!((edge_opacity(1.0, 1.0) - 0.50).abs() < 1e-9);
        // Out-of-range readings clamp instead of over-driving the seam.
        assert!((edge_opacity(-1.0, -1.0) - 0.18).abs() < 1e-9);
        assert!((edge_opacity(4.0, 4.0) - 0.50).abs() < 1e-9);
    }

    #[test]
    fn ac_24_the_edge_light_is_one_static_pixel_in_the_cover_accent() {
        let css = css();
        assert!(css.contains("border: 1px solid @reprise_player_accent"));
        // The cover's radius plus the one pixel the seam sits outside it.
        assert!(css.contains("border-radius: 13px"));
        // Only the alpha moves. A seam whose width or radius changed per frame
        // would throw away the cached node every frame — the same rule the
        // shadow lift follows.
        assert!(!css.contains("transition"));

        // Live spectrum frames and the backdrop's paused breath are the only
        // frame sources; the cover must not own another timer.
        let timer_api = ["add_tick", "callback"].concat();
        assert!(!include_str!("cover_lift.rs").contains(&timer_api));
    }

    #[test]
    fn ac_24_the_edge_light_ignores_the_visualizer_source_switch() {
        // The switch exists for the shadow, which answers the beat inside the
        // Visualizer view. The seam has one formula and no such distinction:
        // switching the source must not move it.
        let lift = CoverLift::headless_for_test();
        lift.feed(0.8, 0.1, 0.5);
        let before = lift.edge_reading();
        lift.set_source(Source::Kick);
        lift.advance_blend(0.4);
        lift.feed(0.8, 0.1, 0.5);
        assert!((lift.edge_reading() - before).abs() < 1e-9);
    }
```

> `edge_reading()` is `#[cfg(test)]` and returns `edge_opacity(pressure, swell)`
> from the stored readings.

- [ ] **Step 2: Run them, see them fail**

```bash
export XDG_CACHE_HOME="$PWD/.cache-test"
cargo test -p reprise-gnome --bins ac_24_the_edge
```

- [ ] **Step 3: Implement**

```rust
/// The mockup's `opacity: calc(0.18 + var(--pres) * 0.10 + var(--sw) * 0.22)`.
const EDGE_REST_OPACITY: f64 = 0.18;
const EDGE_OPACITY_PER_PRESSURE: f64 = 0.10;
const EDGE_OPACITY_PER_SWELL: f64 = 0.22;
/// The seam sits one pixel outside the cover, so it is two pixels wider and
/// its radius is the cover's plus one.
const EDGE_OUTSET_PX: i32 = 1;
const EDGE_CLASS: &str = "reprise-cover-edge-light";

pub(in crate::ui) fn edge_opacity(pressure: f64, swell: f64) -> f64 {
    EDGE_REST_OPACITY
        + EDGE_OPACITY_PER_PRESSURE * pressure.clamp(0.0, 1.0)
        + EDGE_OPACITY_PER_SWELL * swell.clamp(0.0, 1.0)
}
```

`css()` gains, with the radius derived from `RADIUS_SURFACE` so the two cannot
drift — parse its `px` value once and add `EDGE_OUTSET_PX`:

```rust
         .{EDGE_CLASS} {{ border: 1px solid @reprise_player_accent; \
                          border-radius: {edge_radius}px; }}
```

The layer is a `gtk4::Box`, `size_request(width + 2 * EDGE_OUTSET_PX, …)`,
`set_can_target(false)`, `set_can_focus(false)`, **`set_halign(Center)` and
`set_valign(Center)`** — an `Overlay` child defaults to `Align::Fill`, and that
default already produced one shipped bug on this branch (a 51×44 "circle"). Add
it to the root overlay after the cover, so the seam paints on top of the
artwork's edge. Panel cover only (`width == PANEL_WIDTH`), matching the sheen it
replaces; the 56 px bar thumbnail keeps its plain shadow.

`CoverLift` stores `pressure` alongside `swell`/`kick` in `SourceBlend`;
`feed(swell, kick, pressure)` sets all three, `set_swell(swell)` (the player
bar's entry point) leaves `kick` and `pressure` as they are.
`apply_reading_to_widgets` gains
`edge.set_opacity(edge_opacity(pressure, swell))` — note it takes the **raw**
`swell`, not the source-blended `reading` the shadows use.

MOT-7: `swell` is already pinned upstream by `advance_swell`'s
`animations_enabled()` branch, and `pressure` is fed raw — exactly as
`cover_bloom::bloom_opacity` already consumes them. The seam inherits the
backdrop's MOT-7 behaviour by construction. **Do not add a second gate.**

- [ ] **Step 4: Wire it**

In `now_playing.rs`, the two `cover_lift.feed` calls become:

```rust
            self.widgets.cover_lift.feed(0.0, 0.0, 0.0);
```
```rust
        self.widgets.cover_lift.feed(value, self.cover_kick.get(), pressure);
```

- [ ] **Step 5: Write the display regression**

```rust
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn ac_24_the_edge_light_sits_one_pixel_outside_the_cover() {
        // The design puts the seam outside the artwork ("nichts dahinter"),
        // so the assembly is exactly two pixels wider than the cover and the
        // seam is centred on it — an Overlay child left at its default
        // Align::Fill would silently stretch instead.
        …build a panel CoverLift, realize it, measure the rim layer's bounds
        against the cover's, assert width and height differ by exactly 2 and
        that the centres coincide…
    }
```

- [ ] **Step 6: Verify and commit**

```bash
cargo test -p reprise-gnome --bins ac_24_
cargo test --workspace
git add -A
git commit -m "feat(now-playing): light the cover along its edge"
```

---

## Task 3: Split the colour maths out of `cover_accent.rs`

Pure move, no behaviour change. `cover_accent.rs` is at 652 lines and Task 4
adds roughly 200; the split has to happen first so Task 4 stays reviewable.

**Files:**
- Create: `crates/reprise-gnome/src/ui/style/cover_accent_oklab.rs`
- Modify: `crates/reprise-gnome/src/ui/style/cover_accent.rs`
- Modify: `crates/reprise-gnome/src/ui/style/mod.rs` (declare the module)

- [ ] **Step 1: Move**

Move to `cover_accent_oklab.rs`, unchanged: the `Rgb` struct, `to_linear`,
`from_linear`, `linear_rgb_to_oklab`, `oklab_to_linear_rgb`, `scale_chroma`,
`CHROMA_FLOOR`, `CHROMA_CEIL`, `oklch_clamp`, `is_usable`, and the tests
`oklch_clamp_lifts_low_chroma_to_the_floor`,
`oklch_clamp_limits_lightness_and_chroma`, `usable_accepts_vivid_and_rejects_gray`.

Module doc:

```rust
//! OKLab/OKLCH colour maths for the cover palette.
//!
//! Split out of `cover_accent.rs` so the extraction, the provider and the
//! palette each stay reviewable on their own. Nothing here touches GTK or any
//! global state — it is pure arithmetic over sRGB bytes.
```

Visibility: everything the other two modules use becomes
`pub(in crate::ui::style)`, except `Rgb` and `scale_chroma`, which stay
`pub(in crate::ui)` because `waveform_seek.rs` and `player_controller.rs` import
them. Re-export from `cover_accent.rs` so **no call site outside `style/`
changes**:

```rust
pub(in crate::ui) use super::cover_accent_oklab::{scale_chroma, Rgb};
```

- [ ] **Step 2: Verify and commit**

The suite must be green with **zero** test changes beyond the moved ones — that
is the proof this step changed nothing.

```bash
export XDG_CACHE_HOME="$PWD/.cache-test"
cargo test --workspace
bash scripts/check-architecture.sh
git add -A
git commit -m "refactor(style): split the OKLab maths out of the cover accent"
```

---

## Task 4: Three colours instead of one

**Files:**
- Create: `crates/reprise-gnome/src/ui/style/cover_palette.rs`
- Modify: `crates/reprise-gnome/src/ui/style/cover_accent.rs`
- Modify: `crates/reprise-gnome/src/ui/style/mod.rs`
- Modify: `crates/reprise-gnome/src/ui/playback/now_playing_wiring.rs`
- Modify: `crates/reprise-gnome/src/ui/playback/player_controller.rs`
- Modify: `crates/reprise-gnome/src/ui/updates/release_cover.rs`

**Interfaces:**
- Produces: `Palette { primary: Rgb, second: Rgb, third: Rgb }`;
  `dominant_palette(pixels: &[u8], channels: usize) -> Option<Palette>`;
  `accent_from_cover_file(&Path) -> Option<Palette>`;
  `@define-color reprise_player_accent`, `…_accent_2`, `…_accent_3`.

- [ ] **Step 1: Move the extraction into `cover_palette.rs`**

Move `median_cut_buckets`, `dominant_accent`, `accent_from_cover_file`,
`SAMPLE_EDGE` and their tests out of `cover_accent.rs` into the new module.
Module doc:

```rust
//! The three dominant colours of a cover.
//!
//! Median-cut splits the (32 px) cover into eight buckets and scores each by
//! `population × OKLCH chroma`. The old code kept the winner and threw seven
//! away; the palette keeps the best three that are far enough apart in hue to
//! read as different light. The mockup's own words for what these are:
//! "Drei Farben, nach Sättigung gewichtet und auf gleiche Helligkeit gebracht.
//! Near-Black fällt raus — daraus kommt kein Licht."
```

- [ ] **Step 2: Write the failing tests**

```rust
    #[test]
    fn the_palette_keeps_three_distinct_hues_from_one_cover() {
        // A cover with three real colour families plus filler.
        let mut pixels = solid(40, 60, 200, 40);   // blue
        pixels.extend(solid(210, 70, 60, 30));     // red
        pixels.extend(solid(60, 190, 90, 20));     // green
        pixels.extend(solid(20, 20, 24, 40));      // near-black filler
        let palette = dominant_palette(&pixels, 3).expect("three families");
        let hues = [palette.primary, palette.second, palette.third].map(hue_of);
        for (i, a) in hues.iter().enumerate() {
            for b in hues.iter().skip(i + 1) {
                assert!(
                    hue_distance(*a, *b) >= MIN_HUE_SEPARATION,
                    "two palette entries share a hue: {hues:?}"
                );
            }
        }
    }

    #[test]
    fn a_monochrome_cover_still_yields_three_usable_colours() {
        // One vivid family and nothing else. A flat conic sweep of one colour
        // is not a sweep, so the gaps are filled by rotating the primary —
        // never by inventing a colour from outside the artwork's hue.
        let palette = dominant_palette(&solid(200, 60, 40, 64), 3).expect("vivid");
        assert_eq!(palette.primary, palette.primary);
        assert_ne!(palette.second, palette.primary);
        assert_ne!(palette.third, palette.second);
        let base = hue_of(palette.primary);
        assert!(hue_distance(base, hue_of(palette.second)) <= FILL_HUE_STEP + 1e-6);
        assert!(hue_distance(base, hue_of(palette.third)) <= FILL_HUE_STEP + 1e-6);
    }

    #[test]
    fn the_primary_is_exactly_what_the_single_accent_used_to_be() {
        // The player accent is shipped behaviour; three colours must not move
        // the first one.
        let mut pixels = solid(130, 130, 130, 90);
        pixels.extend(solid(220, 40, 40, 10));
        let palette = dominant_palette(&pixels, 3).expect("red cluster");
        assert!(palette.primary.r > 180, "{:?}", palette.primary);
    }

    #[test]
    fn a_grayscale_cover_has_no_palette() {
        assert!(dominant_palette(&solid(128, 128, 128, 64), 3).is_none());
    }
```

- [ ] **Step 3: Implement**

```rust
/// Two palette entries closer than this in OKLCH hue read as one colour.
const MIN_HUE_SEPARATION: f64 = 0.35; // radians, ≈ 20°
/// A monochrome cover's missing entries are filled by rotating the primary
/// this far — enough for the conic sweep to move, small enough to stay the
/// same colour family.
const FILL_HUE_STEP: f64 = 0.38; // radians, ≈ 22°

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) struct Palette {
    pub(in crate::ui) primary: Rgb,
    pub(in crate::ui) second: Rgb,
    pub(in crate::ui) third: Rgb,
}
```

Add to `cover_accent_oklab.rs`:

```rust
/// OKLCH hue of a colour, in radians.
pub(in crate::ui::style) fn hue_of(color: Rgb) -> f64 { … b.atan2(a) … }

/// Shortest angular distance between two hues, in radians (0..=π).
pub(in crate::ui::style) fn hue_distance(a: f64, b: f64) -> f64 { … }

/// `color` rotated by `radians` in OKLCH, keeping L and C.
pub(in crate::ui::style) fn hue_rotated(color: Rgb, radians: f64) -> Rgb { … }
```

`dominant_palette`:

1. Collect opaque pixels and median-cut to 8 buckets exactly as `dominant_accent`
   does today.
2. Score each bucket `population × chroma`, sort descending.
3. Walk the ranking; `oklch_clamp` each average; keep it if it clamps to `Some`
   **and** its hue is at least `MIN_HUE_SEPARATION` from every entry kept so
   far. Stop at three.
4. If nothing was kept, return `None` (grayscale cover — the theme fallback
   applies, unchanged).
5. If one or two were kept, fill from the primary: `second =
   hue_rotated(primary, +FILL_HUE_STEP)`, `third = hue_rotated(primary,
   -FILL_HUE_STEP)`. A two-entry palette fills only `third`.

Delete `dominant_accent`; `accent_from_cover_file` returns
`dominant_palette(...)`.

- [ ] **Step 4: Three colours through the provider and the fade**

In `cover_accent.rs`:

```rust
fn accent_css(palette: Option<Palette>) -> String {
    match palette {
        Some(p) if is_usable(&p.primary) => format!(
            "@define-color reprise_player_accent #{:02x}{:02x}{:02x};\n\
             @define-color reprise_player_accent_2 #{:02x}{:02x}{:02x};\n\
             @define-color reprise_player_accent_3 #{:02x}{:02x}{:02x};",
            p.primary.r, p.primary.g, p.primary.b,
            p.second.r, p.second.g, p.second.b,
            p.third.r, p.third.g, p.third.b,
        ),
        _ => String::new(),
    }
}
```

`accent_during_fade` interpolates all three entries against the theme fallback
(the fallback fills all three — a theme has one accent, and the Schimmer on a
grayscale cover is meant to be a near-flat disc of it). Keep the existing rule
that `new.is_none() && value >= 1.0` clears the override entirely.

Add a test that the fallback path still ends cleared:

```rust
    #[test]
    fn fade_to_theme_fallback_clears_all_three_overrides_at_the_endpoint() {
        let cover = Some(Palette { … });
        let fallback = Rgb { r: 51, g: 201, b: 163 };
        assert!(accent_during_fade(cover, None, fallback, 0.5).is_some());
        assert_eq!(accent_during_fade(cover, None, fallback, 1.0), None);
    }
```

Also extend `accent_css_overrides_when_usable_and_is_empty_otherwise` to assert
all three `@define-color` lines are present.

**The theme stylesheet must define `reprise_player_accent_2` and `_3` too**, so
they resolve before any cover has loaded. Follow whatever the theme does for
`reprise_player_accent` today and give both the same value; the provider
override replaces them per track. If any theme omits them, the Schimmer's CSS
lookup fails at parse time — check every palette.

- [ ] **Step 5: The three call sites**

- `now_playing_wiring.rs`: the stored `Option<Rgb>` becomes `Option<Palette>`;
  `cross_fade_accent` takes palettes. `player_controller.rs:177`'s
  `Rgb as AccentRgb` alias becomes `Palette`.
- `release_cover.rs:23`: takes `.primary`. It wants one colour and still gets
  the same one.

- [ ] **Step 6: Verify and commit**

```bash
export XDG_CACHE_HOME="$PWD/.cache-test"
cargo test --workspace
bash scripts/check-architecture.sh
git add -A
git commit -m "feat(style): extract three dominant colours from the cover"
```

---

## Task 5: The Schimmer

**Files:**
- Create: `crates/reprise-gnome/src/ui/now_playing/cover_shimmer.rs`
- Create: `crates/reprise-gnome/src/ui/now_playing/now_playing_light.rs`
- Modify: `crates/reprise-gnome/src/ui/now_playing/now_playing.rs`
- Modify: `crates/reprise-gnome/src/ui/now_playing/mod.rs`
- Modify: `docs/ux-rules.md` (AC-24, if Task 1's wording needs the Visualizer
  exception spelled out)

### The design

> **Schimmer** — "eine Umdrehung pro Minute"

Band (clips the disc):

```css
position: absolute; left: 0; right: 0; top: 0; height: 340px;
overflow: hidden;
opacity: calc(0.34 + var(--pres) * 0.14 + var(--sw) * 0.16);
```

Disc:

```css
left: 50%; top: 100px; width: 520px; height: 520px; margin: -260px 0 0 -260px;
border-radius: 50%; transform: rotate(var(--rot));
background: conic-gradient(rgb(var(--l1)/.52), rgb(var(--l2)/.40),
                           rgb(var(--l3)/.30), rgb(var(--l2)/.42),
                           rgb(var(--l1)/.52));
mask-image: radial-gradient(circle closest-side, #000 12%, transparent 68%);
```

Read off that CSS, with nothing added:

- The disc's **centre** is at `(panel width / 2, 100 px)` measured from the top
  of the band. The mockup's panel is 302 px wide with a 168 px cover starting at
  22 px padding, so its cover centre is at y = 106 — the disc is centred on the
  cover, 6 px high. Use the design's 100 px.
- Diameter 520 px against a 168 px cover: **the disc is 3.095× the cover's
  width**, and it overflows the panel on both sides on purpose. Express it as
  that ratio so it holds at our panel width, and clip to the band.
- `circle closest-side` on a 520 px box → mask radius **260 px**. Alpha is 1
  out to 12 % of it (31.2 px), falls to 0 at 68 % (176.8 px), and is 0 beyond.
- One turn a minute: `--rot` sweeps 360° in 60 s.
- **It reacts.** The band's opacity is the same shape as the bloom's:
  `0.34 + 0.14·pressure + 0.16·swell`.

**Adaptations, and why:**

- Cairo has no conic gradient. Draw the disc **once per palette** into a cached
  `ImageSurface` as N flat wedges, then apply the mask in a second pass with
  `Operator::DestIn` and a `RadialGradient`. Per frame nothing is built: a
  translate, a rotate, and a `paint_with_alpha`. This is deliberately the same
  bargain `cover_bloom.rs` already makes — buy the raster once a track, spend
  nothing per frame.
- Render the cache at **260 px** and paint it up ×2 with `Filter::Bilinear`.
  The disc is a smooth gradient; the upscale costs nothing visible and quarters
  the rasterization. Again the bloom's own trick.
- The Schimmer runs **outside the Visualizer view only** — the owner asked for
  it there, and that view runs its own light language. Reuse the existing pin:
  when `cover_bloom` is pinned, the Schimmer hides.

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn ac_24_the_shimmer_opacity_matches_the_backdrop_it_lies_on() {
        // Straight from the mockup: 0.34 + 0.14·pres + 0.16·sw.
        assert!((shimmer_opacity(0.0, 0.0) - 0.34).abs() < 1e-9);
        assert!((shimmer_opacity(1.0, 0.0) - 0.48).abs() < 1e-9);
        assert!((shimmer_opacity(1.0, 1.0) - 0.64).abs() < 1e-9);
        assert!((shimmer_opacity(-1.0, 4.0) - 0.50).abs() < 1e-9);
    }

    #[test]
    fn ac_24_the_shimmer_turns_once_a_minute() {
        // "eine Umdrehung pro Minute" — and it must not jump at the wrap.
        assert!((shimmer_angle(0.0) - 0.0).abs() < 1e-9);
        assert!((shimmer_angle(15.0) - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        assert!((shimmer_angle(30.0) - std::f64::consts::PI).abs() < 1e-9);
        assert!((shimmer_angle(60.0) - shimmer_angle(0.0)).abs() < 1e-9);
        assert!((shimmer_angle(61.0) - shimmer_angle(1.0)).abs() < 1e-9);
        // A long session must not lose precision into a stutter.
        assert!((shimmer_angle(86_400.0) - shimmer_angle(0.0)).abs() < 1e-6);
    }

    #[test]
    fn ac_24_the_shimmer_sweeps_the_palette_and_closes_on_itself() {
        let palette = Palette { /* three distinct colours */ };
        // The five stops of the mockup's conic gradient.
        let (r0, g0, b0, a0) = shimmer_stop(palette, 0.0);
        assert!((a0 - 0.52).abs() < 1e-9);
        assert!((shimmer_stop(palette, 0.25).3 - 0.40).abs() < 1e-9);
        assert!((shimmer_stop(palette, 0.50).3 - 0.30).abs() < 1e-9);
        assert!((shimmer_stop(palette, 0.75).3 - 0.42).abs() < 1e-9);
        // A conic gradient wraps: the last stop must equal the first, or the
        // disc shows a seam that rotates once a minute.
        let (r1, g1, b1, a1) = shimmer_stop(palette, 1.0);
        assert!((r0 - r1).abs() < 1e-9 && (g0 - g1).abs() < 1e-9);
        assert!((b0 - b1).abs() < 1e-9 && (a0 - a1).abs() < 1e-9);
    }

    #[test]
    fn ac_24_the_shimmer_mask_is_solid_inside_and_gone_by_two_thirds() {
        // radial-gradient(circle closest-side, #000 12%, transparent 68%)
        assert!((shimmer_mask(0.0) - 1.0).abs() < 1e-9);
        assert!((shimmer_mask(0.12) - 1.0).abs() < 1e-9);
        assert!((shimmer_mask(0.40) - 0.5).abs() < 0.02);
        assert!((shimmer_mask(0.68) - 0.0).abs() < 1e-9);
        assert!((shimmer_mask(1.0) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn ac_24_the_shimmer_disc_is_three_covers_wide() {
        // 520 px against the mockup's 168 px cover.
        assert!((SHIMMER_DIAMETER_PER_COVER - 520.0 / 168.0).abs() < 1e-9);
    }
```

- [ ] **Step 2: Run them, see them fail**

```bash
export XDG_CACHE_HOME="$PWD/.cache-test"
cargo test -p reprise-gnome --bins ac_24_the_shimmer
```

- [ ] **Step 3: Implement `cover_shimmer.rs`**

```rust
//! A slow conic sweep of the cover's three dominant colours, turning behind it
//! once a minute.
//!
//! The colours come from the artwork (`style::cover_palette`), so the light is
//! the record's own — the same honesty rule the bloom follows. Cairo has no
//! conic gradient, so the disc is rasterized once per palette as flat wedges
//! with the radial mask baked in; per frame there is a translate, a rotate and
//! one `paint_with_alpha`. The clock is the backdrop's — this module owns no
//! timer.

const SHIMMER_REST_OPACITY: f64 = 0.34;
const SHIMMER_OPACITY_PER_PRESSURE: f64 = 0.14;
const SHIMMER_OPACITY_PER_SWELL: f64 = 0.16;
/// The mockup's 520 px disc over its 168 px cover.
const SHIMMER_DIAMETER_PER_COVER: f64 = 520.0 / 168.0;
/// Centre of the disc, measured down from the top of the band.
const SHIMMER_CENTRE_Y: f64 = 100.0;
/// One turn a minute.
const SHIMMER_TURN_S: f64 = 60.0;
/// `radial-gradient(circle closest-side, #000 12%, transparent 68%)`.
const SHIMMER_MASK_SOLID: f64 = 0.12;
const SHIMMER_MASK_CLEAR: f64 = 0.68;
/// Edge of the cached raster. Painted up ×2; the disc is a smooth gradient and
/// bilinear costs nothing visible while quartering the rasterization.
const SHIMMER_SURFACE_EDGE: i32 = 260;
/// Wedges in the cached conic. At 260 px this is a 1.4° step — below the
/// resampling filter's own footprint, so no banding survives the upscale.
const SHIMMER_WEDGES: i32 = 256;
```

Pure functions, all testable without GTK:

```rust
pub(super) fn shimmer_opacity(pressure: f64, swell: f64) -> f64 { … }

/// Rotation at `elapsed_s`, wrapped so a long session cannot lose precision.
pub(super) fn shimmer_angle(elapsed_s: f64) -> f64 {
    std::f64::consts::TAU * (elapsed_s / SHIMMER_TURN_S).rem_euclid(1.0)
}

/// The conic gradient's colour and alpha at `t` ∈ [0, 1] around the disc.
/// Five stops: l1 .52, l2 .40, l3 .30, l2 .42, l1 .52 — the last equals the
/// first, because a conic gradient closes on itself.
pub(super) fn shimmer_stop(palette: Palette, t: f64) -> (f64, f64, f64, f64) { … }

/// Mask alpha at `r` ∈ [0, 1] of the disc's radius.
pub(super) fn shimmer_mask(r: f64) -> f64 { … }
```

`CoverShimmer` mirrors `CoverBloom`'s shape deliberately — same field names, same
lifecycle — so the two read as one layer stack:

- `new() -> Self` — a `DrawingArea`, `can_target(false)`, `can_focus(false)`.
- `set_palette(Option<Palette>)` — invalidates the cached surface and redraws.
  Rebuild lazily in `draw`, keyed on the palette like the bloom keys on its
  cover generation. **No palette → the layer draws nothing.** A grayscale cover
  has no light in it; do not substitute the theme accent.
- `set_light(pressure, swell)` — stores both behind the bloom's own
  `LIGHT_EPSILON` (0.01) guard and queues a redraw.
- `set_frame_time(i64)` — stores it and queues a redraw; `0` means "pinned",
  which also resets the stored time so the disc restarts from 0°.
- `set_pinned(bool)` — hides the widget. Called from `sync_bloom_activity`
  with the same flag the bloom uses, so the Schimmer is off in the Visualizer
  view, with the plugin off, and with the panel hidden.

The cached raster, built once per palette:

```rust
// 1. wedges
for i in 0..SHIMMER_WEDGES {
    let t = f64::from(i) / f64::from(SHIMMER_WEDGES);
    let (r, g, b, a) = shimmer_stop(palette, t);
    cr.set_source_rgba(r, g, b, a);
    cr.move_to(cx, cy);
    cr.arc(cx, cy, radius, t * TAU, (t + step) * TAU + OVERLAP);
    cr.close_path();
    cr.fill().ok();
}
// 2. the mask, multiplied into the alpha channel
let mask = cairo::RadialGradient::new(cx, cy, 0.0, cx, cy, radius);
mask.add_color_stop_rgba(0.0, 0.0, 0.0, 0.0, shimmer_mask(0.0));
mask.add_color_stop_rgba(SHIMMER_MASK_SOLID, 0.0, 0.0, 0.0, 1.0);
mask.add_color_stop_rgba(SHIMMER_MASK_CLEAR, 0.0, 0.0, 0.0, 0.0);
mask.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, 0.0);
cr.set_operator(cairo::Operator::DestIn);
cr.set_source(&mask).ok();
cr.paint().ok();
```

> `OVERLAP` is a fraction of a wedge (a hundredth of `step` is plenty) so
> neighbouring wedges share an edge instead of leaving an antialiasing hairline
> between them — 256 hairlines rotating once a minute is exactly the artefact
> this cache is supposed to avoid.

`draw` per frame: clip to the band, translate to
`(width / 2.0, SHIMMER_CENTRE_Y)`, `rotate(shimmer_angle(elapsed_s))`, scale the
cached surface to `SHIMMER_DIAMETER_PER_COVER * NOW_PLAYING_COVER_SIZE`, paint
with `shimmer_opacity(pressure, swell)` and `Filter::Bilinear`.

- [ ] **Step 4: Split `now_playing.rs` before wiring**

`now_playing.rs` is at 769 lines; the wiring below breaches 800. Move the whole
reactive-light fan-out into `now_playing_light.rs` as a second `impl` block:
`advance_swell`, `sync_bloom_activity`, `sync_visual_activity`, and the
`Swell`/`swell_pressure`/`cover_kick`/`swell_last_frame_us` field group's
accessors. Mark the fields the new module reads `pub(super)` —
`now_playing_light` is a sibling under `crate::ui::now_playing`, so `pub(super)`
from `now_playing.rs` reaches it.

Module doc:

```rust
//! The panel's reactive-light fan-out: one reading in, every layer out.
//!
//! Split from `now_playing.rs` to keep both under the file cap. This is the
//! single place that turns a spectrum frame into `pressure` and `swell` and
//! hands them to the cover lift, the backdrop, the shimmer and the readout —
//! having two such places is how a duplicated predicate drifts.
```

- [ ] **Step 5: Wire it**

In `now_playing.rs::build`, construct the shimmer and add it to `head_overlay`
**after the bloom and before everything else**, so the sweep paints over the
blurred cover and under the cover itself.

In `advance_swell` (now in `now_playing_light.rs`), beside the existing calls:

```rust
        self.widgets.shimmer.set_light(pressure, value);
        self.widgets.shimmer.set_frame_time(frame_time_us);
```

and in the `frame_time_us <= 0` reset branch, `set_light(0.0, 0.0)` and
`set_frame_time(0)`.

In `sync_bloom_activity`, beside `bloom.set_pinned(pinned)`:

```rust
        self.widgets.shimmer.set_pinned(pinned);
```

The palette reaches the shimmer wherever the panel already learns about a new
cover — the same place `bloom.set_cover` is called. Pass
`style::cover_palette::accent_from_cover_file`'s result through; do not decode
the cover a second time.

- [ ] **Step 6: Write the display regression**

```rust
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn ac_24_the_shimmer_is_dark_in_the_visualizer_view() {
        // That view runs its own light language; two systems sweeping in
        // different colours against each other is the failure case the pin
        // exists for.
        …build the panel, feed a palette and a live reading, switch to the
        Visual tab, assert the shimmer widget is not visible and that the
        backdrop's frame callback has stopped…
    }
```

- [ ] **Step 7: Verify and commit**

```bash
cargo test -p reprise-gnome --bins ac_24_
cargo test --workspace
bash scripts/check-ux-traceability.sh
bash scripts/check-architecture.sh
git add -A
git commit -m "feat(now-playing): turn a slow conic sweep of the cover's palette behind it"
```

---

## Task 6: Gates

- [ ] **Step 1**

```bash
export XDG_CACHE_HOME="$PWD/.cache-test"
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

- [ ] **Step 2**

```bash
bash scripts/check-ux-traceability.sh
bash scripts/check-motion-tokens.sh
bash scripts/check-architecture.sh
bash scripts/check-accessibility-semantics.sh
bash scripts/check-frontend-thinness.sh
```

`check-architecture.sh` is the one that matters here: three files were at or near
the 800-line cap before this plan started, and Tasks 3, 4 and 5 each carry a
split to stay under it.

- [ ] **Step 3: Commit any fixes**

```bash
git add -A
git commit -m "chore: satisfy the gate battery for the edge light and the shimmer"
```

- [ ] **Step 4: Report, do not tune**

In your summary, list every display test you wrote but did not run, and state
plainly that the bloom and the Schimmer now stack — the mockup shows them as
alternatives, and their combined weight (bloom up to 0.37, Schimmer up to 0.64)
has never been seen together. **Do not pre-emptively lower either.** That is a
judgement to make against the running app, and the reviewer makes it.

---

## Acceptance

The cover reads as an object with light caught along its edge, not as a picture
with something sliding across it. Behind it, a wheel of the record's own three
colours turns slowly enough that you notice it only if you watch — and stops
entirely in the Visualizer view, which has its own light. The blurred cover is
untouched. Nothing built per frame that could be built per track.
