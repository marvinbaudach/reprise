---
slug: the-turning-disc-becomes-visible
worktree: /home/marvin/Projects/reprise-the-turning-disc-becomes-visible
branch: feature/the-turning-disc-becomes-visible
phase: refactored
codex_session:
created: 2026-09-09
---
# The turning disc becomes visible

> **For agentic workers:** implement task by task, test-first, one commit per
> task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Raise the now-playing disc from below the perception threshold to just
above it, with the two constants the measurement says carry the effect —
`SHIMMER_TURN_S` and `SHIMMER_REST_OPACITY` — and fix the phase reset that only
becomes visible once the disc can be seen at all.

**Why:** the owner looked at the panel in use and reported the cover as frozen.
It is not: `cover_shimmer.rs` turns a blurred disc of the artwork behind the
cover, and it was running in the reported state — the Visual tab was visible in
the report, and that tab's visibility hangs off the very same
`song_visuals_active_for_media()` predicate that unpins the disc. It is simply
too quiet to read as motion. Measured, it changes the visible picture by **2.40
of 255 luminance units per second**, which is **0.04×** what the cover bloom's
own breathing does to the same pixels. The complaint is accurate, and the cause
is the tuning, not a bug.

**Register:** the owner chose "just at the edge of vision" over "clearly
visible", and colour that comes only from the artwork. This plan therefore adds
no colour of its own, does not touch the artwork, and adds no preference toggle.

---

## What already exists — read before writing anything

`crates/reprise-gnome/src/ui/now_playing/cover_shimmer.rs` draws a disc of the
blurred cover behind the artwork and rotates it. It owns no timer: its clock
arrives through `NowPlayingPanel::advance_swell` (`now_playing_light.rs:43-78`),
driven by spectrum frames while playing and by the bloom's breath tick at 30 Hz
while paused (`cover_bloom.rs:324-350`).

Geometry, in the artwork band's coordinate space (`tokens.rs`:
`NOW_PLAYING_COVER_SIZE = 168`, `NOW_PLAYING_ARTWORK_BAND = 280`):

| Quantity | Value | Source |
|---|---|---|
| disc diameter | 520 px | `SHIMMER_DIAMETER_PER_COVER * 168` |
| disc radius `R` | 260 px | |
| disc centre, from band top | 100 px | `SHIMMER_CENTRE_Y` |
| cover top edge, from band top | 22 px | `.reprise-now-playing-head { padding: 22px 18px 0 }` |
| cover spans | y ∈ [22, 190] | 168 tall, horizontally centred |
| visible ring | r/R ∈ [0.32, 0.68] | outside the cover, inside the mask |

**The identity this plan rests on.** `shimmer_mask` depends only on `r`, so it is
rotation-invariant. With the disc painted source-over at alpha
`a(r) = shimmer_opacity(p, s) · mask(r)` over a backdrop `B`, the composite is
`a·C(θ) + (1−a)·B`, and therefore

```
Δ(picture) = shimmer_opacity(p, s) · mask(r) · Δ(rotated blurred cover)
```

Three factors that multiply. That is why the levers below compose cleanly, and
why this is measurable without GTK.

### The measurement

`p95` of the per-pixel luminance change **per second** in the visible ring,
averaged over 60 covers sampled from the library's 385 cached 1024 px covers,
with the app's pipeline reproduced exactly (1024 → 32 px box downsample →
bilinear upscale to 260 → radial mask → drawn at 520 px, `paint_with_alpha`).

Control arm: the cover bloom's breathing on the same pixels, same one-second
window, light mode (`0.14 → 0.64` across the free-running 5.5 s swell) — an
effect this app already ships and which reads as motion rather than as flicker.

```
CONTROL — bloom breathing, p95 delta per second:  56.51 / 255

SHIMMER, p95 delta per second (ratio to the bloom in brackets)
  opacity        60s        40s        30s        25s        20s
    0.34      2.40(0.04) 3.39(0.06) 4.25(0.08) 4.82(0.09) 5.55(0.10)
    0.48      3.36(0.06) 4.75(0.08) 5.94(0.11) 6.75(0.12) 7.77(0.14)
```

The harness varied `SHIMMER_MASK_SOLID` rather than the opacity. The two are the
same lever: in the visible ring both masks are linear with the same zero at
`0.68`, so their ratio is the constant `0.56 / 0.40 = 1.40` at every `r`.
Steepening the mask to `0.28` and raising the opacity to `0.48` produce
byte-identical alpha on every visible pixel. **The opacity spelling was chosen**
because it is one constant instead of one plus a token plus an invariant, and
because it leaves the mockup's `radial-gradient(circle closest-side, #000 12%,
transparent 68%)` intact.

Read off the table:

- **Speed is the strongest lever**: 60 s → 25 s is 2.01×, and it is pure motion —
  it changes nothing about how bright the disc rests.
- **Opacity is the second**: 0.34 → 0.48 is 1.40×. It is **not free**: because
  motion and resting brightness share the same factor, the wash around the
  cover becomes 40 % denser. That is a real change to how the panel looks at
  rest, it was put to the owner as such, and it was accepted.
- **Blur resolution was rejected.** 32 → 48 buys 1.14× for a change to
  `cover_glow::blurred_surface`, which the player bar and the bloom also call,
  and 2.25× the cached raster. Kept only as escalation step 2.
- **No combination of constants reaches the bloom.** Everything at maximum is
  still 0.16×. "As obvious as the breathing" is not available from tuning, and
  this plan does not claim it. The target is the threshold, not the bloom.

Honest limits of this evidence:

- The model puts the cover square on the disc's centre; the real cover sits 6 px
  lower. The ring shifts by 6 px, which does not move the ranking.
- `p95` is a peak-ish statistic over the ring, not a mean.
- On the 10 covers with the least structure — the greyscale/near-black case the
  docs name explicitly — the tuned version reaches only 2.58. **On those records
  the disc stays faint, and that is accepted**: the alternative is colour that is
  not in the artwork, which AC-24 rejects on measured grounds.
- Whether 0.12× of the bloom clears the owner's threshold is perceptual. Task 4
  answers it with a screencast; the table only ranks the levers.

---

## Global constraints

- **No new colour, no new layer, no new timer, no new preference.** The existing
  gates stay the only gates: the "Song Visuals" module, `animations_enabled()`
  (MOT-7), the panel's visibility, and AC-26's music test.
- **The artwork itself is not touched.** AC-24: the cover never changes
  brightness, because peripheral luminance change on the cover pulls attention
  off the list. The disc is behind and around it; that is the point.
- **The reactive terms do not move.** `SHIMMER_OPACITY_PER_PRESSURE = 0.14` and
  `SHIMMER_OPACITY_PER_SWELL = 0.16` stay exactly as they are. They deliberately
  mirror the bloom's own reactive slope (`0.15 / 0.16`), which is what
  `ac_24_the_shimmer_opacity_matches_the_backdrop_it_lies_on` is named for. Only
  the resting base changes. The owner asked for ambient life, not more beat.
- **`SHIMMER_MASK_SOLID` and `SHIMMER_MASK_CLEAR` are not touched.**
  `npp_18_shimmer_mask_is_clear_before_the_artwork_band_ends` and
  `ac_24_the_shimmer_mask_is_solid_inside_and_gone_by_two_thirds` must stay green
  **without being edited** — that is the evidence the band is still respected.
- **Clock gaps are deliberately not handled.** While playing, spectrum frames are
  the only clock, so a stall makes the disc catch up in one step. At 25 s a two
  second gap is 29°. Gaps occur at track boundaries, where the cover changes and
  the disc is re-rastered anyway. Decided: leave it.
- **Docs beat code.** AC-24 states the turn rate; amend it first, exactly as
  `docs/plans/edge-light-and-shimmer.md:114` did.
- **No agent attribution in commit messages.**

---

## Task 1: Amend AC-24 first — docs beat code

- [ ] **Step 1: Change the turn rate clause**

In `docs/ux-rules.md`, AC-24, this clause:

> …it lifts on its shadow, carries a one-pixel light seam along its edge, and
> has a soft disc of the blurred artwork turning behind it — one turn a minute.

Replace `one turn a minute` with `one turn every 25 seconds`. Change nothing else
in that sentence.

- [ ] **Step 2: Record why, after the "not colours extracted from it" paragraph**

Add a short paragraph in AC-24's own voice — plain prose, no bullet list, no
tables, matching the surrounding style:

- At one turn a minute the disc changed the visible ring by 2.40 of 255
  luminance units per second, 0.04× the bloom's own breathing on the same
  pixels: below the rate at which it reads as moving at all, which is what was
  reported from use.
- The rate carries most of it and the disc's resting opacity the rest, rising
  from 0.34 to 0.48. **Say plainly that this makes the wash around the cover
  40 % denser at rest**, and that it was accepted for that price — motion and
  resting brightness share one factor here and cannot be bought apart.
- The reactive terms are unchanged and still mirror the backdrop's slope, so the
  music shows through exactly as much as before.
- On greyscale and near-black artwork the disc stays faint. Accepted for the
  same measured reason the palette sweep was rejected in the first place.

- [ ] **Step 3: Commit**

`docs: AC-24 — the disc turns every 25 seconds`. Docs only, no code in this
commit.

---

## Task 2: The two constants

- [ ] **Step 1: Rewrite the two affected tests, and watch them fail**

In `cover_shimmer.rs`'s test module. Both are deliberate rewrites against the
criterion Task 1 just changed, not tests bent to fit new output.

Rename `ac_24_the_shimmer_turns_once_a_minute` to
`ac_24_the_shimmer_turns_every_twenty_five_seconds`, keeping every property the
old one guarded — quarter turn at a quarter period, half at half, no jump at the
wrap, no precision loss after a day:

```rust
#[test]
fn ac_24_the_shimmer_turns_every_twenty_five_seconds() {
    assert!((shimmer_angle(0.0) - 0.0).abs() < 1e-9);
    assert!((shimmer_angle(6.25) - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    assert!((shimmer_angle(12.5) - std::f64::consts::PI).abs() < 1e-9);
    assert!((shimmer_angle(25.0) - shimmer_angle(0.0)).abs() < 1e-9);
    assert!((shimmer_angle(26.0) - shimmer_angle(1.0)).abs() < 1e-9);
    assert!((shimmer_angle(86_400.0) - shimmer_angle(0.0)).abs() < 1e-6);
}
```

Update `ac_24_the_shimmer_opacity_matches_the_backdrop_it_lies_on` to the new
base, keeping its clamping case and its name — the name is about the reactive
slope, which does not change:

```rust
// 0.48 + 0.14·pres + 0.16·sw — the base rose, the slope still mirrors the
// backdrop's own (0.15 / 0.16).
assert!((shimmer_opacity(0.0, 0.0) - 0.48).abs() < 1e-9);
assert!((shimmer_opacity(1.0, 0.0) - 0.62).abs() < 1e-9);
assert!((shimmer_opacity(1.0, 1.0) - 0.78).abs() < 1e-9);
assert!((shimmer_opacity(-1.0, 4.0) - 0.64).abs() < 1e-9);
```

```
cargo test -p reprise-gnome --bins ac_24_the_shimmer
```

Both must fail on the current constants. The two mask tests must be green and
must not be edited.

- [ ] **Step 2: Change the two constants**

```rust
/// One turn every 25 seconds. At a minute the disc changed the visible ring by
/// 2.40 of 255 luminance units per second — 0.04x the bloom's own breathing on
/// the same pixels, below the rate at which it reads as moving at all.
const SHIMMER_TURN_S: f64 = 25.0;
```

```rust
/// Raised from 0.34 with the turn rate: motion and resting brightness share one
/// factor here, so the wash around the cover is 40 % denser at rest. The
/// reactive terms below are deliberately not scaled with it.
const SHIMMER_REST_OPACITY: f64 = 0.48;
```

Fix the module header and any comment still saying "one turn a minute".

- [ ] **Step 3: Verify and commit**

```
cargo test -p reprise-gnome --bins shimmer
```

Green, with both mask tests untouched. `feat: the turning disc turns where it can
be seen`.

---

## Task 3: The disc keeps its phase across a pin

`set_pinned(true)` zeroes `started_at_us` and `frame_time_us`, and
`set_frame_time` re-seeds from zero on the next frame. The disc therefore snaps
back to its starting orientation every time the panel is closed and reopened,
every time the module is toggled, and whenever `animations_enabled()` goes false.
Invisible today; a visible jump after Task 2, which is why it belongs in this
branch. Holding the angle is also the right answer for MOT-7 — with animations
off, a snap to zero *is* a movement.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn npp_18_the_disc_keeps_its_phase_across_a_pin() {
    let shimmer = CoverShimmer::new();
    shimmer.set_pinned(false);
    shimmer.set_frame_time(1_000_000);
    shimmer.set_frame_time(11_000_000);
    let before = shimmer.elapsed_s();
    assert!(before > 0.0, "the disc did not start turning");
    shimmer.set_pinned(true);
    shimmer.set_pinned(false);
    shimmer.set_frame_time(500_000_000);
    let after = shimmer.elapsed_s();
    assert!(
        (after - before).abs() < 1e-6,
        "the disc jumped from {before:.3}s to {after:.3}s across a pin"
    );
}
```

Note the test asserts through a new `pub(super) fn elapsed_s(&self) -> f64`
reader over the accumulated phase, so it checks the value the drawing uses
rather than a private field. `draw` uses that same reader.

- [ ] **Step 2: Implement**

Add `phase_us: Cell<i64>` to `Inner`, holding elapsed time accumulated across
previous running segments. Rename `frame_time_us` to `elapsed_us` — it has
always held elapsed time, and the name has to stop lying now that a second time
field exists.

- Running (`set_frame_time` with a positive frame time and animations enabled):
  seed `started_at_us` when it is zero, then
  `elapsed_us = phase_us + (frame_time_us − started_at_us)`.
- Every stop path — `frame_time_us <= 0`, animations disabled, and
  `set_pinned(true)` — folds the finished segment into `phase_us`, clears
  `started_at_us`, and **leaves `elapsed_us` where it is**, so the angle holds
  instead of snapping to zero. Fold once: a second stop while already stopped
  must not add the segment twice.
- `set_pinned(true)` still resets `pressure` and `swell` to zero. That is the
  "at rest" semantics and it stays.

`shimmer_angle` keeps the `rem_euclid` wrap as the only place the angle is
normalised, so a long session still cannot lose precision.

- [ ] **Step 3: Verify and commit**

```
cargo test -p reprise-gnome --bins shimmer
cargo test -p reprise-gnome --bins now_playing_reactive
```

`now_playing_reactive_tests.rs` drives `set_light`, `set_frame_time` and pinning
directly and must stay green. `fix: the turning disc keeps its phase across a pin`.

---

## Task 4: Prove it moves

The complaint was "I cannot see it move". A green suite cannot answer that, and
neither can the table above.

- [ ] **Step 1: Full gate**

```
cargo test -p reprise-gnome --bins shimmer
cargo clippy -p reprise-gnome --all-targets -- -D warnings
```

Write output to a log under the scratchpad and answer questions with `grep`;
never read a whole log back, and never read a verdict through a pipe.

- [ ] **Step 2: Two screencasts, owner-side**

Recorded by the session that runs this plan, not by Codex. `scripts/showreel/`
`screencast.py <path> <stop-flag> <seconds> <x,y,w,h>` records an area of the
real GNOME session at 30 fps.

The disc turns while **paused** — the bloom's breath tick clocks it at 30 Hz —
so the take needs no audio and no spectrum, and shows exactly the resting state
this change is about. Load a track with structured artwork, pause, open the
now-playing panel, record the artwork band for ~30 s (longer than one turn).

Two arms, same cover, same window geometry:

- **control**: `origin/dev` as it stands — one turn a minute, opacity 0.34
- **fix**: this branch

Deliver both. The question they answer is exactly one: is the disc's motion
visible without being told to look for it, and does it stay on the right side of
"at the edge of vision"?

- [ ] **Step 3: The escalation ladder, if the answer is "still not enough"**

Cheapest first, and no step without the owner's word:

1. `SHIMMER_TURN_S` 25 → 20 s. One constant, +15 %, no blast radius.
2. Parameterise `cover_glow::blurred_surface(texture, edge)`; the shimmer asks
   for 48, the player bar and the bloom keep 32. +14 %, and the only lever that
   adds *structure* rather than amplitude — the one that helps greyscale covers
   most. Costs a shared function and 2.25× the raster.
3. `SHIMMER_REST_OPACITY` above 0.48. Denser wash again; needs its own AC-24
   decision.

---

## Parallelität

**This plan does not split.** Tasks 2 and 3 both rewrite `cover_shimmer.rs` —
Task 3 renames a field that sits beside Task 2's constants — so they share a file
and cannot be two strands. Task 1 owns `docs/ux-rules.md` alone and could
technically run beside them, but cutting a two-paragraph docs change into its own
worktree, branch and PR costs more than it saves, and "docs beat code" wants it
in the same series anyway.

One strand, sequential: 1 → 2 → 3 → 4.

**File ownership** (the whole change):

```
docs/ux-rules.md
crates/reprise-gnome/src/ui/now_playing/cover_shimmer.rs
```

Nothing else is touched. `cover_glow.rs`, `cover_bloom.rs`, `now_playing_light.rs`
and `tokens.rs` stay exactly as they are on `dev`; the escalation ladder's step 2
would change the first of them, and that is deliberately not part of this plan.

**Post-merge cross-checks:** none. No verification step here reads a file the
plan does not own.
