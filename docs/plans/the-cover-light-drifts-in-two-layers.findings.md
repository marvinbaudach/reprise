# Review findings — Two soft clouds drift behind the cover

Reviewed 2026-09-11 against `origin/dev` (795567028e), branch
`feature/the-cover-light-drifts-in-two-layers`. Three Sonnet reviewers
(production Rust, tests, docs) plus one adversarial verification pass on the
one finding whose stated trigger did not hold.

Gates at review time, all green in the rebased worktree: `cargo fmt --check`,
`cargo clippy -p reprise-gnome --all-targets -- -D warnings`, `cargo test -p
reprise-gnome` (2240 + 10 passed, 0 failed), and five display tests each under
its own Xvfb, each reporting `1 passed` rather than being filtered away.

## F1 — The front layer is cropped, not just softer (verified)

`cover_cloud.rs:412` + `:421-428`. `cover_glow::blurred_surface()` has
`BLUR_EDGE = 32` hard-coded and takes no size argument, so `FRONT_BLUR_EDGE =
28` never reaches the rasteriser — it only changes the scale factor
(`320/28 ≈ 11.43`). The 32×32 source is drawn as if it were 28×28, so the
front layer samples only the top-left 87.5 % × 87.5 % of the blurred cover.
`FRONT_BLOBS[1]` at `(0.30, 0.80)` sits in the cut region and therefore takes
its colour from the wrong part of the artwork.

The layer *is* softer, so the intent is half met; the crop is the unintended
half. `cover_bloom.rs`, the sibling consumer of the same helper, never applies
a second edge constant — this is not a house idiom used wrongly, it is new.

Consequence for the docs: the plan's deviation 2 ("die vordere Schicht kommt
aus einem kleineren Raster, 28 statt 32 px") is not true as written.

Untestable as it stands: `npc_15` only compares the two integer constants
against the mockup's `48/54` and never exercises `build_field`.

## F2 — Reduce-motion snaps the clouds to the rest pose (verified)

`cover_cloud.rs:349-361`. `frame_time_us` stores *elapsed* time, so the
`frame_time_us <= 0 || !animations_enabled()` branch zeroing it makes `draw()`
compute `elapsed_s = 0.0` and both layers jump to `drift_at(0, …)`.

The reviewer's stated trigger ("every real pause") is wrong — `CoverBloom`
always passes a non-zero `clock.frame_time()` in both Live and Breathing mode,
and the song-visuals-off path is masked because `set_pinned(true)` hides the
widget in the same synchronous call. The trigger that does hold is the other
half of the condition: `sync_bloom_activity` never consults
`animations_enabled()`, so toggling GNOME's reduce-animation setting at runtime
leaves the widget visible and unpinned, and the next frame snaps it.

Repro: Now Playing open, song visuals on, a track loaded (playback not even
required — the breathing tick suffices), then
`gsettings set org.gnome.desktop.interface enable-animations false`.

The doc comment at `:353-354` promises the opposite ("the composition stays,
the motion stops"). The house pattern is to hold the current pose:
`Swell::value_without_motion()` returns the settled value, `song_visualizer`
calls `snap_to_static()` to freeze the current shape, and the replaced
`cover_shimmer.rs` folded elapsed time into `Phase::hold()` for exactly this.
`CoverCloud` collapsed epoch and elapsed into one `Cell<i64>`, so there is no
carried value left to fold — that is why the only available "freeze" is a
reset.

## F3 — The theme-switch test asserts a tautology (verified)

`now_playing_reactive_tests.rs:93-104`. Both operands are computed *after*
`set_color_scheme("light")`, with identical literal arguments, on a pure
function that takes no theme parameter — the variable named `dark` is simply
misleading. It reduces to `assert_eq!(x, x)`, and it is `#[ignore]`d, so it
spends a display shard doing it. It also hardcodes `16.0` instead of
`BACK_PERIOD_S`.

The test does still assert `cloud_unpinned(&panel)` in the dark theme, so it is
not literally empty — but the property its name and comment claim is not
tested. `CoverShimmer::drawn_angle_for_test()` was deleted with the shimmer and
nothing replaced it, so there is now no way to observe the widget's drawn pose
from a test at all.

## F4 — Nothing asserts a single rendered pixel

The crossfade alphas at `cover_cloud.rs:509`/`:514` could be swapped — the new
cover fading out while the old fades in — and every test would still pass. The
same holds for the `DestIn` masking in `build_field`, the Screen/Multiply
choice, and the several `.ok()`-swallowed `set_source_surface` calls. The only
test that renders anything, `render_cover_cloud_gallery_ppm`, asserts nothing;
it writes a PPM for a human and ends in a `println!`.

F1 and F2 are both instances of this gap.

## F5 — The hold/resume coverage lost with the shimmer

Four tests died with `cover_shimmer.rs` and have no successor:
`npp_18_the_disc_keeps_its_phase_across_a_hold_and_resume`,
`npp_18_a_double_hold_does_not_fold_the_phase_twice`,
`npp_18_resuming_after_a_huge_gap_does_not_jump`,
`npp_18_advance_reports_no_change_when_elapsed_does_not_move`. The new
`started_at_us`/`frame_time_us` state machine has zero unit coverage, which is
why F2 shipped. The other lost tests (pressure-reactive opacity, theme-aware
turn rate, mask alpha shape) are moot by design and are not gaps.

`begin_fade`'s own wiring is likewise untested through the real API — the
`fade_step` decision table is well covered (`npc_23`–`npc_25`, including the
re-change-while-fading case), but nothing drives `set_cover(None)` then
`set_cover(Some)` on a real `CoverCloud` and checks that `leaving_back/front`
were populated.

## F6 — `FRONT_BLOBS[1].radius` disagrees with the plan and its own test comment

Code says `0.40` (`cover_cloud.rs:117-122`); the plan (`:58`) and the test
comment (`cover_cloud_tests.rs:227`) both say 45 %. `npc_16` asserts the
`BACK_BLOBS` radii and never the front ones, so the suite cannot see it. The
plan also claims the radii "bleiben wörtlich wie spezifiziert", which this
contradicts. Needs a decision either way — the design canvas is not in the
repo.

## F7 — Stale shimmer references in live rules

- `docs/ux-rules.md:4348` — "The backdrop and **the disc** rest when …", two
  sentences after the same block was rewritten to "drifting clouds". Unchanged
  context in the diff, so the rewrite missed it.
- `docs/ux-rules.md:4358` — AC-26, `[active]`, still reads "the cover bloom and
  **shimmer** driven by the session's own artwork".
- `now_playing_light.rs:98` — comment still says "hid the turning disc".

Repo-wide search found nothing else live; the plan documents that keep the
shimmer as history are correctly left alone.

## F8 — Blur radii in the plan contradict themselves

The plan gives three different accounts of the same two constants: 32 px for
both layers (`:22`), 48/54 px (`:51`/`:56`), and 34/38 px via the `/240`
conversion table (`:43-44`). The code has 32 and 28. The code's own comment
(ratio, not absolutes) is the accurate account; the conversion table is not.

## F9 — Per-frame work the module header says it avoids

`cover_cloud.rs:573-588` rebuilds the 25-stop scrim gradient and re-parses a
hex colour through `accent::sidebar_background_rgb()` on every draw, while the
header (`:14-17`) advertises that nothing is re-rasterised when the clock
moves. The blob surfaces indeed are not; the scrim is. Theme-scoped cache would
fix it.

## F10 — `render_cover_cloud_gallery_ppm` rides the display gate

It is tagged `#[ignore = "requires a display; run via xvfb-run"]`, so
`scripts/check-display-tests.sh` picks it up in every shard sweep, although it
asserts nothing and exists to produce an operator artifact. The script exempts
tests tagged `#[ignore = "measurement: …"]`, which is what this one is. No
false green — it cannot go red — just avoidable CI cost.

## Nit

`docs/ux-rules.md`'s "the pair holds no repeated pose before 80 seconds" is
true of the pair's starting pose only; `drift_progress` folds a triangle wave
through `smoothstep`, so individual poses recur by mirror symmetry much earlier
(`drift_at(5 s)` = `drift_at(75 s)`). "The period is 80 s" is the defensible
claim. The plan makes the same imprecise statement.

## Not a review question

The clouds are markedly subtler than the mockup — the direct consequence of the
decision to take colour from the blurred cover instead of an extracted palette.
The lever is blob opacity (0.60/0.55 back, 0.45/0.40 front), not the colour
source. Nobody has seen the real panel with a real cover; the evidence so far
is the PPM gallery from `render_cover_cloud_gallery_ppm`. That is an owner
judgement, not a finding.
