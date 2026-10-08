---
slug: swipe-visualizer-handover-a
worktree: /home/marvin/Projects/reprise-swipe-visualizer-handover-a
branch: feature/swipe-visualizer-handover-a
phase: refactored
codex_session:
created: 2026-10-07
---
# Strand A (Rust): the engine holds and hands over without a dip

Mother plan: `swipe-visualizer-handover.md`. Read its Problem, Goal and Decisions first. This
strand implements decisions 4–7 and the AC-29 text.

## File ownership

- `crates/reprise-core/src/playback/cava/**`
- `crates/reprise-core/src/visuals/engine.rs` and its tests
- `crates/reprise-android-ffi/src/visualizer*.rs` and `crates/reprise-android-ffi/src/live_audio.rs`
- the AC-29 section of `docs/ux-rules.md`

Touch nothing else. No Kotlin.

## Tasks (test first: write the failing test, run it, see it fail, implement, run the gates)

1. **Hold through a reset before the flip (decision 4).**
   - Test in `visualizer_swipe_hold_tests.rs`: feed live frames, then `reset_audio_stream`, then
     read frames with no `note_track_changed` in between.
   - Expected: the displayed bars equal the last live shape, with at most one gravity step of
     decay.
   - If the test passes on the current code, keep it as a pin and change nothing.
2. **No overshoot on a gain rise (decision 6).**
   - Test in the `boundary_*_tests` beside `playback/cava/smoothing.rs`: a loud song, a carry,
     then a quiet song with `M > gain × CARRY_BAND`.
   - Expected: no frame's tallest bar exceeds `TARGET_HEIGHT` by more than the existing brake
     tolerance.
   - Fix it in `smoothing.rs:97-118`, by ordering the brake after the rise or clamping the risen
     gain at `M`, whichever keeps the existing boundary tests green.
3. **Hold the adopted shape while `Waiting` (decision 5).**
   - Test in `visualizer_shape_continuity_tests.rs`: `adopt_shape` with a loud shape, then a
     quiet stream.
   - Expected:
     - The displayed mean never drops below the quiet stream's own settled mean before it
       reaches that mean.
     - The hold ends by the first decided window.
     - A `Done` within `CARRY_BAND` releases at the carried gain.
   - Implement it as a display hold keyed on `has_adopted_shape` plus the boundary phase. Do not
     add a second gain path.
   - The existing `adopted_shape_holds_until_the_live_stream_speaks` and
     `adopt_shape_keeps_bars_on_screen_through_the_first_live_pcm_block` must stay green. Adjust
     them only where their expectation is exactly the decay this task removes, and say so in the
     commit.
4. **Caps follow the morph (decision 7).**
   - Test in the `engine.rs` tests: after an adoption, while the held shape moves down into the
     new song's bars, no cap stays more than one segment (1/16) above its bar for longer than the
     morph.
   - Outside a handover, cap fall is unchanged. Pin it with a test that normal playback still
     decays caps at `PEAK_FALL`.
5. **AC-29 text.** In the "Swipes on the phone" paragraph of `docs/ux-rules.md`, add:
   - a neighbour card shows the live shape while it is dragged and while it settles;
   - from release to the new song's bars, the shape does not dip below the new song's level and
     does not overshoot;
   - caps follow the handover.

   Follow the document's amendment procedure, and name the tests from tasks 1–4. Strand B's
   Kotlin test names are added after the merge (mother plan, post-merge check 1). Do not name
   them here.

## Gates

`cargo fmt --check`, `cargo clippy --all-targets --workspace -- -D warnings`,
`cargo test --workspace`, `cargo audit`, and the core purity check from AGENTS.md. Every code file
stays under 800 lines; extract a sibling module rather than trimming docs.
