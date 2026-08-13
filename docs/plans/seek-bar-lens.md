# The seek bar, completed: the lens comes back — quantised

> **For agentic workers:** implement task by task, test-first, one commit per
> task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Finish the three-layer treatment at the playhead. **Two of the three
layers already exist on this branch and are correct — do not rebuild them.**
The missing one is the lens: the bars around the playhead swell with the bass.

**Read this before anything else.** The lens was already shipped once on this
branch and taken back out because it flickered
(`116a46f73a feat(seek): swell the waveform around the playhead with the bass`,
reverted by `5e53196cb7 refactor(visuals): take the reactive light back out of
the waveform and the track list`). There is a guard test in the tree today whose
sole job is to stop it coming back. This task deliberately retires that guard —
which is only defensible because all three causes of the old flicker are named,
fixed, and pinned by a sharper test each. If you cannot make those three tests
pass, **stop and report**; do not delete the guard and ship anyway.

---

## What already exists — verified in the tree, do not touch

| Spec layer | Status | Where |
|---|---|---|
| **1 — Playhead-Schein** | **done, exact** | `waveform_seek_render.rs:122–148` |
| **2 — Linse** | **missing** | this plan |
| **3 — Gespielte Kante** | **done, exact** | `waveform_seek_render.rs:127, 152–159` |

- `playhead_glow_radius` = `height * (0.22 + 1.15 * kick)`, `playhead_glow_alpha`
  = `0.30 + 0.12 * pressure + 0.40 * kick`, both clamped, drawn **after** the
  bars, `Operator::Add`, clipped to the widget rectangle. That is the spec's §1
  line for line.
- `glow_is_active(fill_bars, drag_fraction, build_progress, crossfade_progress)`
  already stands the glow down for the mini player, during a drag, during
  build-up and during a crossfade. The lens reuses **this exact predicate** —
  do not write a second one. (Two copies of one decision is how a duplicated
  predicate drifts, and it has already cost this project twice.)
- `played_alpha` = `0.55 + 0.45 * (1 - distance)`, purely positional. That is
  §3 line for line.
- `set_bass(kick, pressure)` in `waveform_seek.rs:461` already passes both
  readings through `motion::reactive_amplitude`, so MOT-7 is handled **at the
  entry point**. Everything downstream inherits it. Do not add a second gate.

### One deliberate deviation from the spec, already in the tree

The spec asks for "zwei konzentrische Ebenen … ihre Opazität überblenden" rather
than a per-frame blur, because "ein Gauß-Blur mit wechselndem Radius verwirft den
gecachten Node in jedem Frame". That rule is about GTK's CSS/GSK shadow nodes —
it is why `cover_lift.rs` cross-fades two static `box-shadow` layers. The seek
bar is not a styled widget: it is a `DrawingArea` with a Cairo draw function.
A `cairo::RadialGradient` is a pattern built and consumed inside a single
`draw`; there is no node to invalidate and nothing to cache between frames. The
shipped one-gradient implementation is therefore both simpler and cheaper than
the two-layer construction, and it satisfies the rule's intent. **Leave it
alone.**

---

## Global constraints

- **Worktree:** `~/Projects/reprise-reactive-light`, branch
  `feature/reactive-light`. Do not touch files outside it. Do not push.
- `cargo test -p reprise-gnome --bins <filter>` — it is a binary crate.
- **Do not run display tests and do not start Xvfb.** Write them, leave them
  `#[ignore = "requires a display; run via xvfb-run"]`, name them in your
  summary, continue.
- `export XDG_CACHE_HOME="$PWD/.cache-test"` before any test run.
- **Known-red on this base — not yours:**
  `browse_bar::…widget_projects_removable_chips_without_a_redundant_reset_button`
  and `song_visualizer::…bars_fullscreen_render_budget_diagnostic` (a timing
  diagnostic that fails under machine load).
- **Do not touch:** `shape_display_peaks`, `aggregate_rms`, `smooth_neighbors`
  and the percentile/gamma constants in `reprise-view` — the stored waveform is
  the truth and the lens is presentation, applied *after* shaping.
  Also off limits: `bass_pressure.rs` (no new core reading), the track list,
  `eq_bars.rs`, and the cover/panel/play-button breathing from round 5 (those
  run on `swell`, not `kick`).
- `waveform_seek_tests.rs` is at 649 lines against a 800-line cap. If your tests
  push it over, split the new ones into
  `waveform_seek_lens_tests.rs` rather than trimming coverage.
- One commit per task, no attribution footer.

---

## Task 1: `kick_soft` — the soft driver

`kick` drives the light; `kick_soft` drives the bars. Two time constants on one
surface are the point: the light flashes a moment before the wave peaks, which
reads as a strike with a tail. Formed **in the UI** — this is not a new core
reading.

**Files:**
- Modify: `crates/reprise-gnome/src/ui/player_bar/waveform_seek.rs`

**Interfaces:**
- Produces: `kick_soft_step(previous: f64, kick: f64, dt_s: f64) -> f64`;
  `State.kick_soft: f64`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn ac_24_the_soft_kick_strikes_at_once_and_leaves_over_220_ms() {
    use super::kick_soft_step;
    // Attack is instant — the strike sits on the beat, not after it.
    assert!((kick_soft_step(0.0, 1.0, 0.016) - 1.0).abs() < 1e-9);
    assert!((kick_soft_step(0.3, 0.9, 0.016) - 0.9).abs() < 1e-9);
    // Release is linear over 220 ms: a full kick is gone after 220 ms and
    // exactly half gone after 110 ms.
    assert!((kick_soft_step(1.0, 0.0, 0.110) - 0.5).abs() < 1e-9);
    assert!((kick_soft_step(1.0, 0.0, 0.220) - 0.0).abs() < 1e-9);
    // It never falls below the reading that is still arriving …
    assert!((kick_soft_step(0.6, 0.5, 1.0) - 0.5).abs() < 1e-9);
    // … and never below zero, however long the gap between frames.
    assert!((kick_soft_step(1.0, 0.0, 99.0) - 0.0).abs() < 1e-9);
    // Out-of-range readings clamp instead of driving the bars past full.
    assert!((kick_soft_step(0.0, 4.0, 0.016) - 1.0).abs() < 1e-9);
    assert!((kick_soft_step(-1.0, -1.0, 0.016) - 0.0).abs() < 1e-9);
}
```

- [ ] **Step 2: Run it, see it fail**

```bash
export XDG_CACHE_HOME="$PWD/.cache-test"
cargo test -p reprise-gnome --bins ac_24_the_soft_kick
```

- [ ] **Step 3: Implement**

```rust
/// Release window of `kick_soft`. `kick` itself falls in 70 ms, which reads as
/// a blink on a surface this wide — that was one of the three reasons the first
/// lens flickered.
const KICK_SOFT_RELEASE_S: f64 = 0.220;

/// Attack immediately, release linearly. Deliberately not an exponential: a
/// linear tail reaches zero, so the tick callback below can actually settle.
pub(super) fn kick_soft_step(previous: f64, kick: f64, dt_s: f64) -> f64 {
    let kick = kick.clamp(0.0, 1.0);
    let previous = previous.clamp(0.0, 1.0);
    if kick > previous {
        kick
    } else {
        (previous - dt_s.max(0.0) / KICK_SOFT_RELEASE_S).max(kick)
    }
}
```

Add `kick_soft: f64` to `State`, initialised `0.0`.

- [ ] **Step 4: Drive it from the existing tick — and let it keep the tick alive**

This is the part that is easy to get wrong. The tick callback
(`ensure_tick_callback`, ~line 569) **stops itself when everything has settled**.
Three consequences, all of which must be handled:

1. In the `animations_enabled()` branch, advance `kick_soft` from the same `dt`
   the interpolation already computes:
   ```rust
   s.kick_soft = kick_soft_step(s.kick_soft, s.bass_kick, dt / 1_000_000.0);
   ```
   `dt` there is in microseconds; do not reuse it without converting.
2. In the `else` branch (animations off), pin `s.kick_soft = 0.0` alongside the
   other pinned fields.
3. Extend the `settled` condition with `&& s.kick_soft <= 0.0`. Without this the
   tick can stop mid-decay and leave the bars frozen mid-swell — playback
   pausing calls `set_bass(0.0, 0.0)` (`player_bar.rs:299`), and if the position
   interpolation has already settled there is nothing left to run the tail out.

And in `set_bass`, when the new reading is non-zero, call
`self.ensure_tick_callback()` so a kick arriving on a settled bar starts the
clock that will later release it. Update its doc comment: it no longer feeds
only the glow.

- [ ] **Step 5: Write the display regression**

```rust
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn ac_24_a_kick_on_a_settled_bar_runs_its_tail_out() {
        // The tick stops itself when the position, the build and the crossfade
        // have settled. A kick arriving after that has to restart it, or the
        // bars freeze mid-swell — which is exactly what pausing does.
        …build a WaveformSeek, settle it, call set_bass(1.0, 1.0), pump the main
        context past 220 ms, assert the state's kick_soft is back to 0.0…
    }
```

- [ ] **Step 6: Verify and commit**

```bash
cargo test -p reprise-gnome --bins ac_24_the_soft_kick
cargo test --workspace
git add -A
git commit -m "feat(seek): add kick_soft, the soft driver for the waveform lens"
```

---

## Task 2: The lens

**Files:**
- Modify: `crates/reprise-gnome/src/ui/player_bar/waveform_seek_render.rs`
- Modify: `crates/reprise-gnome/src/ui/player_bar/waveform_seek_tests.rs`
- Modify: `docs/ux-rules.md` (AC-24)

**Interfaces:**
- Produces: `lens_growth(bar_h, index, count, fraction, kick_soft, scale, max_bar_height) -> f64`
  and `lensed_bar_height(...) -> f64`.

### Why it flickered last time, and what each fix is

Read the reverted commit if you want the detail; here is the summary, and each
line becomes a test below.

| Cause | Then | Now |
|---|---|---|
| Window half as wide | `LENS_SIGMA_SQUARED = 30.0` → σ ≈ 3.9, about eight bars twitching individually — the eye reads that as a defect | `LENS_SIGMA_SQ = 128.0` → σ = 8, about thirty bars moving together |
| Driver too sharp | raw `kick` / `impact`, 70 ms release — a blink | `kick_soft`, 220 ms release |
| Sub-pixel edges | `LENS_MIN_GROWTH_PX = 0.5`, a threshold that does not fix the edges at all | growth quantised to **even** device pixels |

**The even-pixel rule is the whole reason this holds.** Bars are anchored on
their centre, so a growth of 1 px puts 0.5 px on the top edge and 0.5 px on the
bottom — both land on half-pixels, and *that* was the flicker, even at whole
bar heights. Only even steps keep both edges on the grid. `scale` is
`gtk_widget_get_scale_factor`, so this holds on HiDPI too.

A bar therefore grows by 0, 2, 4 or 6 px and nothing between. With thirty bars
at different starting heights the steps fall at different moments: one bar
jumps, the field swells.

- [ ] **Step 1: Retire the old guard, with its reason recorded**

Replace `ac_24_bar_heights_are_identical_at_every_reading` (currently at
`waveform_seek_tests.rs:43`) — do not simply delete it. In its place:

```rust
/// The lens is back. The guard that used to stand here forbade *any* height
/// movement, because the first lens flickered. It is replaced — not dropped —
/// by the three tests below, one per cause of that flicker: a window wide
/// enough to read as a wave, a driver soft enough to be one, and growth
/// quantised to even device pixels so no edge ever lands on a half-pixel.
```

Delete `bar_height_for_test_with_kick`, the stub that ignores its `_kick`
argument, and point the new tests at `lensed_bar_height` — **the same function
`draw_bars` calls**. A pure helper that merely resembles the drawing path is how
a widget ends up 51 px wide while its test asserts 44; do not reintroduce that
seam.

- [ ] **Step 2: Write the failing tests**

```rust
#[test]
fn ac_24_lens_growth_lands_only_on_even_device_pixels() {
    // Bars are centre-anchored: an odd growth splits into two half-pixel
    // edges, which is what flickered. Sweep the whole reading range and every
    // bar around the playhead, at 1x and at 2x.
    for &scale in &[1.0, 2.0] {
        let step = 2.0 * scale;
        for k in 0..=100 {
            let kick_soft = f64::from(k) / 100.0;
            for index in 0..120 {
                let base = bar_height_for_test(180, 3.9, 26.0);
                let grown = lensed_bar_height(base, index, 120, 0.5, kick_soft, scale, 26.0);
                let grow = grown - base;
                assert!(grow >= 0.0, "the lens must never shrink a bar");
                let steps = grow / step;
                assert!(
                    (steps - steps.round()).abs() < 1e-9,
                    "growth {grow} is not a multiple of {step} (scale {scale}, kick {kick_soft})"
                );
                assert!(grown <= 26.0, "the lens grew past the ceiling: {grown}");
            }
        }
    }
}

#[test]
fn ac_24_a_still_reading_leaves_the_waveform_bit_identical() {
    // At rest the stored waveform is shown exactly as `shape_display_peaks`
    // produced it — the lens is presentation, not data.
    for level in [0u8, 40, 128, 200, 255] {
        let base = bar_height_for_test(level, 3.9, 26.0);
        for index in 0..120 {
            assert_eq!(
                lensed_bar_height(base, index, 120, 0.5, 0.0, 1.0, 26.0),
                base,
                "a bar moved at kick_soft = 0"
            );
        }
    }
}

#[test]
fn ac_24_the_lens_is_a_wave_not_a_twitch() {
    // σ = 8 bars: about thirty move together. The first lens used σ ≈ 3.9 and
    // read as eight bars twitching. Measure the count that actually moves.
    let base = bar_height_for_test(255, 3.9, 26.0);
    let moved = (0..240)
        .filter(|&i| lensed_bar_height(base, i, 240, 0.5, 1.0, 1.0, 200.0) > base)
        .count();
    assert!(moved >= 24, "only {moved} bars move — that is a twitch");
    // And it is symmetric: the lens shows where you are, not what was.
    let head = 120usize;
    for offset in 1..=10 {
        assert_eq!(
            lensed_bar_height(base, head - offset, 240, 0.5, 1.0, 1.0, 200.0),
            lensed_bar_height(base, head + offset, 240, 0.5, 1.0, 1.0, 200.0),
            "the lens is lopsided at ±{offset}"
        );
    }
    // Beyond the cutoff nothing moves at all.
    assert_eq!(lensed_bar_height(base, 0, 240, 0.5, 1.0, 1.0, 200.0), base);
}
```

- [ ] **Step 3: Run them, see them fail**

```bash
export XDG_CACHE_HOME="$PWD/.cache-test"
cargo test -p reprise-gnome --bins ac_24_lens ac_24_a_still_reading ac_24_the_lens_is
```

- [ ] **Step 4: Implement**

```rust
/// σ = 8 bars (the exponent carries the conventional 2σ²), so roughly thirty
/// bars move together and the result reads as a wave. The first lens used 30.0
/// here — σ ≈ 3.9 — and read as eight bars twitching.
const LENS_SIGMA_SQ: f64 = 128.0;
/// Maximum growth. The Visualizer runs at about 100 %; this is a fifth of it,
/// because the waveform still has to be readable while it moves.
const LENS_GAIN: f64 = 0.18;
/// Past this the Gaussian is worth less than one device pixel anyway.
const LENS_CUTOFF_BARS: f64 = 24.0;

/// Growth of one bar, quantised to whole **even** device pixels.
///
/// Bars are centre-anchored, so an odd growth splits into two half-pixel edges
/// — that, and not the height itself, is what made the first lens flicker.
/// The headroom below the ceiling is floored to the same even step, so a bar
/// that runs into `max_bar_height` still sits on the grid.
pub(super) fn lens_growth(
    bar_h: f64,
    index: usize,
    count: usize,
    fraction: f64,
    kick_soft: f64,
    scale: f64,
    max_bar_height: f64,
) -> f64 {
    if count == 0 {
        return 0.0;
    }
    let d = index as f64 - fraction * count as f64;
    if d.abs() > LENS_CUTOFF_BARS {
        return 0.0;
    }
    let lens = (-(d * d) / LENS_SIGMA_SQ).exp();
    let raw = bar_h * LENS_GAIN * kick_soft.clamp(0.0, 1.0) * lens;
    let step = 2.0 * scale.max(1.0);
    let grow = (raw / step).round() * step;
    let headroom = ((max_bar_height - bar_h).max(0.0) / step).floor() * step;
    grow.min(headroom)
}

pub(super) fn lensed_bar_height(
    bar_h: f64,
    index: usize,
    count: usize,
    fraction: f64,
    kick_soft: f64,
    scale: f64,
    max_bar_height: f64,
) -> f64 {
    bar_h + lens_growth(bar_h, index, count, fraction, kick_soft, scale, max_bar_height)
}
```

In `draw_bars`, after the existing `bar_h` match and **only** for
`DisplayBar::Level`:

```rust
        let bar_h = if lens_active {
            lensed_bar_height(
                bar_h, index, count, state.fraction, state.kick_soft, scale,
                state.max_bar_height,
            )
        } else {
            bar_h
        };
```

- `DisplayBar::Silence` keeps its fixed 2 px dot. Silence that pulses is a lie.
- `lens_active` is the **existing** `glow_is_active(state.fill_bars,
  state.drag_fraction, state.build_progress, state.crossfade_progress)`. Rename
  it to `reactive_light_is_active` and update its two call sites and its test —
  it now gates both layers, and one predicate is the point. While the user is
  dragging, light under the finger is in the way; during build-up and crossfade
  another animation owns the bar.
- `scale` comes from `area.scale_factor()` in `draw`; thread it through
  `BarDrawStyle` (it is already the per-draw parameter bag) rather than
  reaching for the widget inside `draw_bars`.
- The mini player is excluded by `fill_bars` inside that same predicate: σ = 8
  would cover nearly half of its 46 bars.

- [ ] **Step 5: Amend AC-24**

Add to the AC-24 bullet about the seek bar:

```markdown
  The bars around the playhead swell with the bass over a window wide enough
  to read as one wave, and their growth is quantised to even device pixels so
  no bar edge ever lands on a half-pixel. The swell, the playhead light and
  the played-bar gradient all stand down while the user is dragging the
  playhead, during build-up and during a track crossfade.
```

- [ ] **Step 6: Verify and commit**

```bash
cargo test -p reprise-gnome --bins ac_24_
cargo test --workspace
bash scripts/check-ux-traceability.sh
git add -A
git commit -m "feat(seek): swell the bars around the playhead on even pixels"
```

---

## Task 3: The rest-state regressions

**Files:**
- Modify: `crates/reprise-gnome/src/ui/player_bar/waveform_seek_tests.rs`
  (or `waveform_seek_lens_tests.rs` if the file cap forces a split)

- [ ] **Step 1: Write them**

```rust
#[test]
fn ac_24_both_layers_stand_down_together() {
    use super::render::reactive_light_is_active;
    assert!(reactive_light_is_active(false, None, 1.0, 1.0));
    assert!(!reactive_light_is_active(false, Some(0.4), 1.0, 1.0), "drag");
    assert!(!reactive_light_is_active(false, None, 0.5, 1.0), "build");
    assert!(!reactive_light_is_active(false, None, 1.0, 0.5), "crossfade");
    // The mini player is 46 bars wide; a σ = 8 lens would cover half of it,
    // and the glow would light the whole thing.
    assert!(!reactive_light_is_active(true, None, 1.0, 1.0), "mini");
}

#[test]
fn ac_24_silence_dots_never_pulse() {
    // `DisplayBar::Silence` is a fixed 2 px dot at every reading. Silence that
    // breathes is a lie about the recording.
    …drive `draw_bars`' height selection for a Silence bar across the whole
    kick_soft range and assert SILENCE_DOT_HEIGHT every time…
}
```

- [ ] **Step 2: Verify and commit**

```bash
cargo test -p reprise-gnome --bins ac_24_
git add -A
git commit -m "test(seek): pin the rest states of the playhead light and lens"
```

---

## Task 4: Gates

- [ ] **Step 1**

```bash
export XDG_CACHE_HOME="$PWD/.cache-test"
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

- [ ] **Step 2**

```bash
bash scripts/check-motion-tokens.sh
bash scripts/check-architecture.sh
bash scripts/check-accessibility-semantics.sh
bash scripts/check-frontend-thinness.sh
bash scripts/check-ux-traceability.sh
```

Core purity: `reprise-core` and `reprise-view` must be untouched by this plan.
`git diff --name-only origin/dev...HEAD -- crates/reprise-core crates/reprise-view`
must be empty for this task's commits.

- [ ] **Step 3: Commit any fixes**

```bash
git add -A
git commit -m "chore: satisfy the gate battery for the waveform lens"
```

---

## If it looks restless

In this order, one quantity at a time — and **never** loosen the quantisation,
which is the only thing standing between this and the version that was already
reverted once:

1. `LENS_GAIN` → 0.12
2. `KICK_SOFT_RELEASE_S` → 0.320
3. `LENS_SIGMA_SQ` → 200 (σ ≈ 10 bars)

---

## Acceptance

A wide field of bars swells around the playhead with the bass, reading as a wave
rather than individual twitching strokes, with a light above it that opens on
the beat. Hold two still frames side by side and the bar heights differ only in
even pixel steps. The movement is clearly weaker than the Visualizer's: the
waveform stays readable and you can still hit a spot in the track. Grab the
playhead and it goes quiet at once.
