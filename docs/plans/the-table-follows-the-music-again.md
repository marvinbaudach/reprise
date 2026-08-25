---
slug: the-table-follows-the-music-again
worktree:
branch:
phase: planned
codex_session:
created: 2026-08-25
strands: a,b
merge_order: a,b
---
# The table follows the music again

Fixes the root causes measured in
[`the-table-stops-following-the-music.md`](the-table-stops-following-the-music.md).
That document carries the probe log excerpts and database values; nothing
here re-derives them.

The user's explicit requirement is **regression tests, so this cannot break
again**. That shapes every task, because the existing display tests would
not have caught any of these. All of them produce a *correct* intermediate
value and a wrong final one. Asserting `adjustment.value()` at the end sees
the wrong number without knowing who wrote it, and goes green the moment
any writer happens to land last. The oracle is therefore the **ordered list
of writers** from `scroll_probe::trail`. `search_viewport_display_tests.rs`
already does this and owns the `viewport_steps()` helper — reuse it rather
than writing a second one.

## The three causes, and what each fix is

**A — the reveal is overwritten by the view-state restore.**
`route_to_place` centres in `refresh_and_select` and then restores the
remembered anchor in `restore_browser_place`, which wins by running last.
The codebase already holds the fact that would prevent it:
`ScrollGlide::destination()` is "where the viewport is actually headed",
and `capture_reload_anchor` reads it for exactly this reason. But it lives
only while an animation runs, and the centring path uses `jump_to`, which
cancels the animation and leaves `destination() == None`. The fix keeps the
intent in `ScrollGlide` past a jump. No new state in `Shared` — a second
place to record the same decision is the drift this repo has been bitten by
before.

**B — the remembered row height can be confirmed but never contradicted.**
Two named defects, not one:

1. `track_list_reload.rs:617` schedules the re-measurement under
   `if is_queue`. The flat library list therefore never plans one at all;
   it only ever re-measures when `reload_anchor_scroll` happens to run.
2. When it does re-measure, `remember_after_layout` reads `upper` *after*
   the preseed wrote it from the stale value. `settled_row_height` requires
   `upper / n_rows` to agree with the widget measurement and does **nothing**
   when they disagree — so the measurement can only ever confirm the
   remembered value. Measured: remembered 53, allocated 45, `upper`
   pendulums `90270 ↔ 106318` for the whole run.

The fix: a uniform widget measurement above the CSS floor is stronger
evidence than an `upper` we seeded ourselves. When it contradicts the
remembered height, it replaces it.

**C — the source lists read "the user is scrolling" from `value-changed`.**
Proven by inspection against the track table's own documented fix. Podcasts,
YouTube and Radio all mark themselves as user-scrolled on every programmatic
write, so `ChangedElsewhere` degrades to `MarkerOnly` and the list stops
following playback.

**C's second half is not in this plan.** `set_playing_episode` also runs
`self.render()` before any policy decision and without pinning the
viewport — the candidate for the reported YouTube double-click jump. It is
unmeasured, and the podcast scroller carries no probe at all, which is why
the reproduction run that played YouTube episodes produced no scroll lines.
Strand B instruments it and records the finding; the fix is a follow-up plan
written once its oracle is known.

## Control arm

Every new test gets a **mutation probe when it is written**: revert the fix
in production code, confirm the test goes red, paste that output into the
strand file as its acceptance evidence, then discard the reversion. No
`cfg(test)` switch survives in the production path — a second branch that
can itself be wrong is not a control arm, it is more surface.

## Out of scope, deliberately

- `RevealMotion::Glide` never sets a GTK list anchor via `scroll_to` while
  `Instant` does. This looked like a fourth cause and is not one: without a
  model swap there is no allocation pass to re-anchor, and every measured
  jump followed a reload.
- The nine `SCROLL JUMP-TO-TOP` records are a symptom of B's pendulum.
  Expect them to go with B; if they do not, that is a new investigation
  with new evidence, not a reason to widen this one.

## Verification

Per strand as listed in its file. The local gate list comes from
`merge-readiness`, never hand-assembled.

Closing check, after both strands land — the reported symptom is visible,
not measurable: play a track, scroll away, click the player-bar title, and
watch the view stay on the track. The tests keep it fixed; this says it is
fixed at all.

## Parallelität

Two strands. The cut is real: the file groups are disjoint, and the one
file that could have been contested (`scroll_glide.rs`) is settled by
keeping C's unmeasured half out of this plan.

**Strand A — the track table.** Causes A and B. They are one strand and
must not be split: A's stand-down is expressed in adjustment values and B
changes which values the geometry produces, so split apart each would be
written against the other's unfixed behaviour.

Owns:
- `crates/reprise-gnome/src/ui/track_list/**`
- `crates/reprise-gnome/src/ui/list_geometry*.rs`
- `crates/reprise-gnome/src/ui/scroll_glide.rs`
- `crates/reprise-gnome/src/ui/scroll_center.rs`
- `crates/reprise-gnome/src/ui/view_session.rs`
- `crates/reprise-gnome/src/ui/window/library_shell.rs`
- `crates/reprise-core/src/library/settings_geometry.rs` and the migration

**Strand B — the source lists.** Cause C's proven half, plus the
measurement for its unproven half.

Owns:
- `crates/reprise-gnome/src/ui/podcasts/**`
- `crates/reprise-gnome/src/ui/radio/**`
- `crates/reprise-gnome/src/ui/source_reveal.rs`
- `crates/reprise-gnome/src/ui/scroll_probe.rs`

`scroll_probe.rs` goes to **B**, which extends it with the podcast writers.
Strand A only *reads* the existing `trail` API, which is already there and
needs no change. If A turns out to need a new probe point, that is a
post-merge follow-up, not a mid-flight ownership change.

**Merge order: A, then B.** Not a code dependency — B's views do not touch
`list_geometry` at all. A goes first because A is what the closing manual
check exercises, so the user-visible symptom becomes confirmable before B's
larger surface arrives.

**Post-merge cross-checks.** Neither strand can make these alone:

1. The closing manual check above. It exercises the track table (A) after
   the source views (B) have changed the inputs to the shared
   `source_reveal` policy. A's strand cannot run it, because the policy
   change is B's.
2. `source_reveal::reveal_policy` serves three views while the track table
   keeps its own `current_track_selection::reveal_policy`. Confirm after
   both merges that the two still agree on the 1.5-second grace, or that
   any divergence is deliberate and written down. Each strand sees one half.
3. The full display suite, once. Both strands change scroll behaviour the
   suite covers, and the interesting failure is the one needing both
   changes present.
