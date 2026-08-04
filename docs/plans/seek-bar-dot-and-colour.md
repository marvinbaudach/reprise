# The seek bar reacts without fluttering: a dot and a colour

> **For agentic workers:** implement task by task, test-first, one commit per
> task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Give the seek bar back a reaction to the music without touching a
single bar height. Two layers: a **dot** on the playhead that carries the beat,
and a **colour** on the played part that carries the pressure.

**Read this first — the history is the design rationale.** Reactive light has
been tried on this surface three times and rejected three times:

1. A lens swelling the bars around the playhead (σ ≈ 3.9, raw `kick`).
2. The same lens rebuilt properly — σ = 8, a 220 ms `kick_soft`, growth
   quantised to even device pixels. Still rejected on sight.
3. A soft radial glow on the playhead, riding `kick`. Rejected as blinking.

The owner's diagnosis after the third attempt, and the premise of this plan:

> Unruhe entsteht nicht durch zu viel Bewegung, sondern wenn **Nachbarbalken
> sich unterschiedlich verändern**. […] Die Rate ist das Problem, nicht die
> Stärke — jede Amplitudenreduktion ergibt nur ein leiseres Flattern.

So: **the bar geometry is not touched at all**, and the only thing allowed to
ride the raw beat is a single element, because a single element cannot shimmer —
shimmering needs neighbours drifting apart. Everything else moves by the same
amount everywhere at once.

---

## What exists right now — verified in the tree

The branch has just been rolled back to a clean baseline; do not assume anything
from earlier rounds is still there.

- `waveform_seek_render.rs` (319 lines) carries **no** bass signal. The playhead
  is a 1 px line in the accent at `PLAYHEAD_ALPHA = 0.70`, drawn after the bars
  over the full `max_bar_height`.
- `waveform_seek.rs` (634 lines) has **no** bass state at all — no `bass_kick`,
  no `kick_soft`. `WaveformSeek::set_bass` does not exist.
- `played_alpha(index, count, fraction)` = `0.55 + 0.45 * (1 - distance)` is
  already there and already correct — that is the plan's §3, unchanged.
- Unplayed bars are **white** at `UNPLAYED_ALPHA = 0.18`; played bars are the
  accent at `played_alpha`. Ghost (drag) `0.40`, hover preview `0.30`.
- `glow_is_active(fill_bars, drag_fraction, build_progress, crossfade_progress)`
  does **not** exist any more either; it went with the glow. Task 2 brings the
  predicate back under a name that says what it now gates.
- `ac_24_the_waveform_reads_neither_bass_signal` is the current guard: it asserts
  the render source contains neither `kick` nor `pressure`. **Task 1 replaces
  it** — read that task before touching anything.
- `PlayerBar::set_bass(_kick, pressure)` already computes `swell` for the cover
  lift and currently drops `kick` on the floor. It is the one place that has all
  three readings.
- `motion::reactive_amplitude` was deleted when its last consumer went. Do not
  resurrect it; gate MOT-7 inline where the single consumer is.

---

## Global constraints

- **Worktree:** `/home/marvin/Projects/reprise-reactive-light`, branch
  `feature/reactive-light`. Do not touch files outside it. Do not push.
- `cargo test -p reprise-gnome --bins <filter>` — it is a binary crate.
- **Do not run display tests and do not start Xvfb.** Write them, leave them
  `#[ignore = "requires a display; run via xvfb-run"]`, name them in your
  summary, continue.
- `export XDG_CACHE_HOME="$PWD/.cache-test"` before any test run.
- **Known-red on this base — not yours:**
  `browse_bar::…widget_projects_removable_chips_without_a_redundant_reset_button`.
- **Do not touch:** `shape_display_peaks`, `aggregate_rms`, `smooth_neighbors`
  and the percentile/gamma constants in `reprise-view`; **any bar height**;
  `bass_pressure.rs`; the track list; `eq_bars.rs`; the cover, panel and
  play-button treatment.
- `waveform_seek_tests.rs` is at 649 lines against an 800-line cap. If your tests
  push it over, split the new ones into `waveform_seek_light_tests.rs` rather
  than trimming coverage.
- **A source-text assertion must not name the symbol it forbids.**
  `include_str!` reads the test file too, so a literal like
  `assert!(!source.contains("fn shimmer_stop"))` always finds itself. Split the
  needle: `["fn shimmer", "_stop"].concat()`. This has already cost two
  debugging rounds on this branch.
- One commit per task, no attribution footer.

---

## Task 1: Swap the guard — heights, not signals

The current guard forbids the *words* `kick` and `pressure` in the render. This
round deliberately introduces both. The property that actually has to hold is
narrower and stronger: **no bar height may move.**

**Files:**
- Modify: `crates/reprise-gnome/src/ui/player_bar/waveform_seek_tests.rs`
- Modify: `crates/reprise-gnome/src/ui/player_bar/waveform_seek_render.rs`

**Interfaces:**
- Produces: `bar_height_for_test_with_light(level, min, max, kick, pressure, swell) -> f64`,
  which must call **the same function `draw_bars` calls** to pick a height.

- [ ] **Step 1: Write the failing test**

Replace `ac_24_the_waveform_reads_neither_bass_signal` with:

```rust
/// The regression that keeps the lens out. Three attempts died here: a narrow
/// lens, a wide lens quantised to even device pixels, and a playhead glow.
/// The first two moved bar heights and read as noise in the rounding error,
/// because neighbouring bars crossed their pixel boundary at different
/// moments. This round reacts in colour and in one dot instead, so the
/// property to pin is not "no signal reaches this file" — both readings now
/// do — but "no height moves, whatever they say".
#[test]
fn ac_24_bar_heights_never_move_with_the_music() {
    use super::render::{bar_height_for_test, bar_height_for_test_with_light};
    let (min, max) = (3.9, 26.0);
    for level in [0u8, 40, 128, 200, 255] {
        let reference = bar_height_for_test(level, min, max);
        for step in 0..=20 {
            let reading = f64::from(step) / 20.0;
            for (kick, pressure, swell) in [
                (reading, 0.0, 0.0),
                (0.0, reading, 0.0),
                (0.0, 0.0, reading),
                (reading, reading, reading),
            ] {
                assert_eq!(
                    bar_height_for_test_with_light(level, min, max, kick, pressure, swell),
                    reference,
                    "a bar height moved at kick {kick}, pressure {pressure}, swell {swell}"
                );
            }
        }
    }
}
```

- [ ] **Step 2: Implement**

`bar_height_for_test_with_light` takes the readings, ignores them, and returns
whatever `draw_bars`' own height selection returns. Extract that selection into
a named function if it is still inline, so the test and the drawing cannot
diverge — a helper that merely *resembles* the drawing path is how this project
once shipped a 51×44 "circle" under a passing test.

- [ ] **Step 3: Verify and commit**

```bash
export XDG_CACHE_HOME="$PWD/.cache-test"
cargo test -p reprise-gnome --bins ac_24_bar_heights
git add -A
git commit -m "test(seek): pin bar heights against the readings, not against the words"
```

---

## Task 2: The colour on the played part

This is the layer that carries the pressure. **Every played bar changes by the
same amount at the same moment**, so there is nothing that can drift apart.

**Files:**
- Modify: `crates/reprise-gnome/src/ui/player_bar/waveform_seek.rs`
- Modify: `crates/reprise-gnome/src/ui/player_bar/waveform_seek_render.rs`
- Modify: `crates/reprise-gnome/src/ui/player_bar/player_bar.rs`

**Interfaces:**
- Produces: `played_light(pressure: f64, swell: f64) -> f64`;
  `WaveformSeek::set_bass(kick: f64, pressure: f64, swell: f64)`;
  `State.bass_kick`, `State.bass_pressure`, `State.bass_swell`.

### The numbers

```
alpha = 0.74 + 0.16 * pressure + 0.10 * swell        // the colour term
final = alpha * played_alpha(index, count, fraction)  // × the existing position term
```

**The constant term is the important one.** The boundary between played and
unplayed must never depend on the audio: on a quiet passage the position would
become unreadable, and a seek bar whose progress you cannot see is broken. 0.74
is the floor; the signal adds at most 0.26 on top.

`swell` is the slow envelope from round 5 (a lagging `pressure` crossed with a
free-running 5.5 s breath). Its share is small and exists only so the played
part is not perfectly still on an even track.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn ac_24_the_played_colour_rides_a_floor_that_never_moves() {
    use super::render::played_light;
    // The floor is what keeps the progress boundary readable on a quiet track.
    assert!((played_light(0.0, 0.0) - 0.74).abs() < 1e-9);
    assert!((played_light(1.0, 0.0) - 0.90).abs() < 1e-9);
    assert!((played_light(0.0, 1.0) - 0.84).abs() < 1e-9);
    assert!((played_light(1.0, 1.0) - 1.00).abs() < 1e-9);
    // Out-of-range readings clamp instead of over-driving the fill.
    assert!((played_light(-1.0, -1.0) - 0.74).abs() < 1e-9);
    assert!((played_light(4.0, 4.0) - 1.00).abs() < 1e-9);
    // Never below the floor, at any reading in range.
    for step in 0..=20 {
        let reading = f64::from(step) / 20.0;
        assert!(played_light(reading, reading) >= 0.74 - 1e-9);
    }
}

#[test]
fn ac_24_the_progress_boundary_is_legible_in_silence() {
    // Measured requirement, not a matter of taste: at pressure = swell = 0 the
    // played part must differ from the unplayed part by at least 3:1 in
    // luminance, so the boundary reads without relying on hue — which is what
    // a red/green-blind user, or a glance, actually has.
    //
    // Composite each side over the bar's own background and compare relative
    // luminance (WCAG: (L1 + 0.05) / (L2 + 0.05)).
    let played = composited_luminance(accent_rgb(), played_light(0.0, 0.0) * 1.0);
    let unplayed = composited_luminance((1.0, 1.0, 1.0), UNPLAYED_ALPHA);
    let ratio = (played.max(unplayed) + 0.05) / (played.min(unplayed) + 0.05);
    assert!(ratio >= 3.0, "played/unplayed luminance ratio is only {ratio:.2}:1");
}
```

> Write `composited_luminance(rgb, alpha)` and `accent_rgb()` as test helpers:
> alpha-composite over the theme's bar background (`style::theme`'s default
> palette) and return WCAG relative luminance. Use the *brightest* played bar
> (position term 1.0, i.e. at the playhead) — that is the bar the eye uses to
> find the boundary.
>
> **If the ratio is below 3:1, lower `UNPLAYED_ALPHA` until it passes. Do not
> raise the accent.** The accent also fills the play button, where a brighter
> value glares. Say in your summary what you changed it to and why.

- [ ] **Step 2: Implement**

```rust
/// The colour term of a played bar: a floor plus what the music adds.
///
/// The floor is not tuning. The played/unplayed boundary is the seek bar's
/// primary information, and a boundary that dims with a quiet passage makes
/// the position unreadable exactly when the listener looks for it.
const PLAYED_LIGHT_FLOOR: f64 = 0.74;
const PLAYED_LIGHT_PER_PRESSURE: f64 = 0.16;
const PLAYED_LIGHT_PER_SWELL: f64 = 0.10;

pub(super) fn played_light(pressure: f64, swell: f64) -> f64 {
    PLAYED_LIGHT_FLOOR
        + PLAYED_LIGHT_PER_PRESSURE * pressure.clamp(0.0, 1.0)
        + PLAYED_LIGHT_PER_SWELL * swell.clamp(0.0, 1.0)
}
```

In `draw_bars`, the played branch becomes
`style.opacity * played_light(state.bass_pressure, state.bass_swell) * played_alpha(index, count, state.fraction)`.
Ghost, hover-preview and unplayed branches are untouched.

`WaveformSeek::set_bass(kick, pressure, swell)` stores all three behind the
existing 0.01 epsilon guard and queues a redraw. **MOT-7 gates `kick` only** —
inline, `if motion::animations_enabled() { kick } else { 0.0 }`. The colour is a
colouring, not an animation, so `pressure` and `swell` pass through as they are;
this mirrors what `cover_bloom` already does with the same two readings.

In `player_bar.rs::set_bass`, hand all three on:
`self.waveform.set_bass(kick, pressure, value)` where `value` is the swell it
already computes. Take the parameter name back off its underscore.

- [ ] **Step 3: Verify and commit**

```bash
cargo test -p reprise-gnome --bins ac_24_
cargo test --workspace
git add -A
git commit -m "feat(seek): warm the played part with the bass without moving a bar"
```

---

## Task 3: The dot on the playhead

**Files:**
- Modify: `crates/reprise-gnome/src/ui/player_bar/waveform_seek_render.rs`
- Modify: `docs/ux-rules.md` (AC-24)

**Interfaces:**
- Produces: `playhead_dot_radius(kick) -> f64`, `playhead_dot_halo(kick) -> f64`,
  `playhead_dot_alpha(kick) -> f64`, `reactive_light_is_active(...) -> bool`.

### The numbers (36 px widget height is the reference)

```
radius = 5.0 + 7.0  * kick
blur   = 4.0 + 20.0 * kick
spread = 3.0 * kick
alpha  = 0.55 + 0.45 * kick
```

**It replaces the 1 px playhead line — it is not drawn in addition to it.**

A single element cannot shimmer; shimmering needs neighbours drifting apart.
That is the whole reason this one layer may ride the raw `kick` and swing hard:
even at six kicks a second it stays a pulsing dot rather than noise.

**Adaptation, stated because it departs from the spec's letter.** The spec asks
for two concentric layers cross-faded rather than a per-frame blur, because a
blur radius that changes every frame throws away GTK's cached node. That rule is
about GTK's CSS/GSK shadow nodes — it is why `cover_lift.rs` cross-fades two
static `box-shadow` layers. The seek bar is a `DrawingArea` with a Cairo draw
function: a `cairo::RadialGradient` is built and consumed inside one `draw`,
there is no node to invalidate, and nothing survives between frames to cache.
So draw a solid core circle of `radius` plus a `RadialGradient` halo out to
`radius + blur`, both in one pass. The identical construction shipped as the
round-4 glow and measured inside the 60 Hz budget.

The other three conditions from the spec hold as written:

- **Additive or `screen`.** An opaque dot swallows the bars it sits on.
- **Clip to the widget**, or half of it runs off the edge at the track's start
  and end.
- **Rest value while dragging.** Whoever is looking for a spot needs a clear
  view of the thing they are grabbing.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn ac_24_the_playhead_dot_swings_hard_because_one_element_cannot_shimmer() {
    use super::render::{playhead_dot_alpha, playhead_dot_halo, playhead_dot_radius};
    assert!((playhead_dot_radius(0.0) - 5.0).abs() < 1e-9);
    assert!((playhead_dot_radius(1.0) - 12.0).abs() < 1e-9);
    assert!((playhead_dot_halo(0.0) - 4.0).abs() < 1e-9);
    assert!((playhead_dot_halo(1.0) - 24.0).abs() < 1e-9);
    assert!((playhead_dot_alpha(0.0) - 0.55).abs() < 1e-9);
    assert!((playhead_dot_alpha(1.0) - 1.00).abs() < 1e-9);
    // Out-of-range readings clamp.
    assert!((playhead_dot_radius(9.0) - 12.0).abs() < 1e-9);
    assert!((playhead_dot_alpha(-1.0) - 0.55).abs() < 1e-9);
}

#[test]
fn ac_24_the_dot_stands_down_where_it_would_be_in_the_way() {
    use super::render::reactive_light_is_active;
    assert!(reactive_light_is_active(false, None, 1.0, 1.0));
    assert!(!reactive_light_is_active(false, Some(0.4), 1.0, 1.0), "drag");
    assert!(!reactive_light_is_active(false, None, 0.5, 1.0), "build");
    assert!(!reactive_light_is_active(false, None, 1.0, 0.5), "crossfade");
    // The mini player is 46 bars wide; a dot with a 20 px halo would light
    // half of it.
    assert!(!reactive_light_is_active(true, None, 1.0, 1.0), "mini");
}
```

- [ ] **Step 2: Implement**

The dot is drawn where the line is drawn today, after the bars, clipped to the
widget rectangle, with `cairo::Operator::Add`. When
`reactive_light_is_active(...)` is false, draw it at its rest values
(`radius = 5.0`, `alpha = 0.55`, **no halo**) rather than not at all — it is the
playhead, and the playhead must not disappear while the user drags it.

**The mini player gets no dot and keeps its 1 px line.** Task 2's colour applies
there; this does not.

- [ ] **Step 3: Amend AC-24**

The seek-bar sentence currently says nothing in the waveform reads a bass
signal. Replace that clause with:

```markdown
  The waveform's **bar heights** never move: three attempts to swell them
  around the playhead were rejected, because neighbouring bars cross their
  pixel boundary at different moments and the eye reads that as noise rather
  than as life. What reacts instead is colour and one dot. The played part
  takes a floor plus what the bass adds, so every played bar changes by the
  same amount at the same instant and the progress boundary stays legible
  at any volume — it keeps at least a 3:1 luminance ratio against the
  unplayed part in silence. The playhead itself is a dot that rides the raw
  beat, which it may do precisely because a single element cannot shimmer.
  It falls back to its rest size while the user drags it, during build-up
  and during a track crossfade; the mini player has no dot at all.
```

- [ ] **Step 4: Write the display regression**

```rust
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ac_24_animations_off_freezes_the_dot_but_keeps_the_colour() {
    …with gtk-enable-animations=false, feed a full kick and assert the stored
    reading is 0 (rest radius, no halo) while the played colour still follows
    pressure — the colour is a colouring, not an animation…
}
```

- [ ] **Step 5: Verify and commit**

```bash
cargo test -p reprise-gnome --bins ac_24_
cargo test --workspace
bash scripts/check-ux-traceability.sh
git add -A
git commit -m "feat(seek): put a pulsing dot on the playhead in place of its line"
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

Core purity: `git diff --name-only origin/dev...HEAD -- crates/reprise-core
crates/reprise-view` must contain nothing from this plan's commits.

- [ ] **Step 3: Commit any fixes**

```bash
git add -A
git commit -m "chore: satisfy the gate battery for the seek bar's dot and colour"
```

---

## Acceptance

On a metal track a dot pulses at the playhead in time, and the waveform stands
**absolutely still** — hold two frames side by side and not one bar height
differs. The played part visibly warms on dense material and settles on a quiet
one, without the progress boundary ever becoming hard to find. Grab the playhead
and you have a clear view of it.
