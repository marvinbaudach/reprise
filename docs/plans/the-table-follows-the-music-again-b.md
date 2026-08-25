---
slug: the-table-follows-the-music-again-b
worktree: /home/marvin/Projects/reprise-the-table-follows-the-music-again-b
branch: feature/the-table-follows-the-music-again-b
phase: shipped
codex_session:
created: 2026-08-25
---
# Strand B — the source lists learn what a user scroll is

Mother plan:
[`the-table-follows-the-music-again.md`](the-table-follows-the-music-again.md).
Evidence: [`the-table-stops-following-the-music.md`](the-table-stops-following-the-music.md).

## File ownership

Touch only these:

- `crates/reprise-gnome/src/ui/podcasts/**`
- `crates/reprise-gnome/src/ui/radio/**`
- `crates/reprise-gnome/src/ui/source_reveal.rs`
- `crates/reprise-gnome/src/ui/scroll_probe.rs`

Strand A owns the track list, `list_geometry*`, `scroll_glide.rs` and
`scroll_center.rs`. Do not touch them, and do not fix anything you notice
there — report it instead.

## Task 1 — scroll activity comes from input, not from the adjustment

`podcasts_view_marker.rs:31` records scroll activity from
`vadjustment().connect_value_changed`. The track table fixed exactly this
and wrote down why (`track_list_builder.rs`, the NAV-10b comment): every
reload, every anchor restore and the centring glide itself write that
value, so the list marks itself as user-scrolled after every reload and
`LoadedItemChange::ChangedElsewhere` degrades to `MarkerOnly`.

Replace it with the track table's construction:

- `EventControllerScroll` with `EventControllerScrollFlags::BOTH_AXES`,
  `PropagationPhase::Capture`, returning `Propagation::Proceed`;
- a `GestureDrag` in the capture phase on the vertical scrollbar.

Apply to **all three** source views — Podcasts, YouTube and Radio
(`radio_reveal.rs:209` holds the same `Cell<Option<Instant>>`). They share
`source_reveal.rs` for the policy, so they must share the input source too.
Extract the wiring into one helper; three copies is how they drift back
apart.

Read the track table's version before writing this one — its comment
explains why the scrollbar is watched through a separate gesture rather
than one gesture over the whole scroll area (a drag there competes with the
rows' own `DragSource`).

**Test 1a**: a programmatic write to the adjustment does not mark the list
as user-scrolled; a synthesized scroll event does. Assert through
`source_reveal::reveal_policy` staying `Reveal` after the programmatic
write and turning `MarkerOnly` after the event.

**Test 1b**: all three views wire the same helper. A view added later
without it is the regression this test exists to catch.

## Task 2 — measure the double-click jump, do not fix it

The user reports that double-clicking a YouTube episode moves the list.
`PodcastsView::set_playing_episode` runs `self.render()` — a full rebuild —
*before* deciding any policy and without pinning the viewport. The track
table pins exactly this class of mutation
(`now_playing_marker::reapply_now_playing_markers_pinned`, written for the
"double-click jumps the table and snaps back" report). That is a candidate,
not a finding.

The reproduction run that played YouTube episodes produced **no scroll
lines at all**, because the podcast scroller carries no probe.

1. Extend `scroll_probe` to the podcast scroller. Name its writers the way
   the track list's are named, so one trail is readable across both.
2. Reproduce the double-click with `REPRISE_SCROLL_PROBE=1` and record what
   the viewport does: whether `render()` moves it, and whether the move is
   ours (a named writer) or GTK's (an `Observed` entry no writer claims).
3. Write the finding into this file under "Measurement result", with the
   log excerpt. State plainly if it turns out `render()` does *not* move
   the viewport — that is a result, and it redirects the follow-up.

**Do not write the fix.** Its oracle is unknown until step 2, and a test
written before the measurement encodes the guess instead of the behaviour.
The follow-up plan is written from this result.

## Control arm

Task 1's tests each get a mutation probe: revert the production change, run
the test, confirm it fails, paste the output under "Acceptance evidence",
discard the reversion.

Task 2 has no control arm because it has no fix — its evidence is the log
excerpt itself.

## Verification

- Task 1's tests with their mutation probes recorded.
- The gate list from `merge-readiness`.
- The closing manual check belongs to the mother plan's post-merge list.

## Acceptance evidence

### Test 1a — input, not adjustment movement

Mutation: temporarily replaced the shared capture-phase scroll controller and
scrollbar gesture with the former `vadjustment().connect_value_changed`
callback.

```text
thread 'ui::source_reveal::tests::src_13_only_source_list_input_marks_the_user_as_scrolling' panicked at crates/reprise-gnome/src/ui/source_reveal.rs:107:9:
assertion `left == right` failed: a programmatic adjustment write is not user input
  left: MarkerOnly
 right: Reveal
test ui::source_reveal::tests::src_13_only_source_list_input_marks_the_user_as_scrolling ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2832 filtered out
```

After restoring the production input wiring, the focused `source_reveal`
slice passed all seven tests, including the synthesized scroll signal.

### Test 1b — all three source views share the helper

Mutation: temporarily restored only Radio's former adjustment callback while
leaving the common Podcasts/YouTube view on the shared helper.

```text
thread 'ui::source_reveal::tests::src_13_podcasts_youtube_and_radio_use_the_shared_input_wiring' panicked at crates/reprise-gnome/src/ui/source_reveal.rs:157:13:
assertion `left == right` failed: Radio must wire the shared source-list input helper exactly once
  left: 0
 right: 1
test ui::source_reveal::tests::src_13_podcasts_youtube_and_radio_use_the_shared_input_wiring ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2833 filtered out
```

After restoring Radio's shared-helper call, the structural test passed.

## Measurement result

`render()` did **not** move the YouTube viewport in the isolated reproduction.
The run built an 80-episode YouTube source, expanded its complete episode
window, scrolled to a visible row, and invoked the same `podcasts.play` action
the row's double-click dispatches. The activation reached
`set_playing_episode`, whose before/after snapshots bracket its full
`self.render()`. After another 500 ms of main-loop settlement the adjustment
was still exactly where it started:

```text
SCROLLOBSERVED scope=episode-list from=0.0 to=2117.5 upper=4228.0 page=378.0
SCROLLSNAPSHOT at=measurement.before-double-click value=2117.5 upper=4228.0 page=378.0
SCROLLSNAPSHOT at=episode-marker.render.before value=2117.5 upper=4228.0 page=378.0
SCROLLSNAPSHOT at=episode-marker.render.after value=2117.5 upper=4228.0 page=378.0
SCROLLSNAPSHOT at=measurement.after-double-click value=2117.5 upper=4228.0 page=378.0
```

The only `Observed` movement is the harness's deliberate initial scroll. No
named writer and no unclaimed GTK observation appears between the before and
after double-click snapshots. This falsifies `set_playing_episode`'s grouped
list `render()` as the cause in this reproduction; the follow-up should probe
the installed view's actual pointer sequence and the YouTube channel-detail
surface rather than adding the track table's pinning fix here.
