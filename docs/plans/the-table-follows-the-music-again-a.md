---
slug: the-table-follows-the-music-again-a
worktree: /home/marvin/Projects/reprise-the-table-follows-the-music-again-a
branch: feature/the-table-follows-the-music-again-a
phase: coded
codex_session:
created: 2026-08-25
---
# Strand A — the track table follows the music again

Mother plan:
[`the-table-follows-the-music-again.md`](the-table-follows-the-music-again.md).
Evidence: [`the-table-stops-following-the-music.md`](the-table-stops-following-the-music.md).
Read both before starting; do not re-derive the measurements.

## File ownership

Touch only these:

- `crates/reprise-gnome/src/ui/track_list/**`
- `crates/reprise-gnome/src/ui/list_geometry*.rs`
- `crates/reprise-gnome/src/ui/scroll_glide.rs`
- `crates/reprise-gnome/src/ui/scroll_center.rs`
- `crates/reprise-gnome/src/ui/view_session.rs`
- `crates/reprise-gnome/src/ui/window/library_shell.rs`
- `crates/reprise-core/src/library/settings_geometry.rs` + its migration

`scroll_probe.rs` belongs to strand B. Read its `trail` API; do not change it.

## Task 1 — the reveal outranks the restore

`ScrollGlide` already answers "where is the viewport headed" through
`destination()`, and `track_list_reload::capture_reload_anchor` reads it for
precisely this purpose. The answer disappears after a jump: `jump_to`
cancels the animation, and `destination()` is derived from a *running*
animation only. `centered_scroll_restore::write_centered` jumps.

1. Give `ScrollGlide` a durable intent: the last value deliberately placed,
   whether by `glide_to` (the animation's target) or by `jump_to` (the value
   written). `destination()` keeps its current meaning for callers that want
   "an animation is in flight"; the new accessor answers "the viewport holds
   a deliberate destination".
2. Clear the intent when it stops being true: a real user scroll (the
   capture-phase `EventControllerScroll` and the scrollbar `GestureDrag` in
   `track_list_builder.rs` are the existing seams — use them, not
   `value-changed`), a row activation, and a foreign write the glide already
   detects via `foreign_write`.
3. Make both restore paths respect it while the adjustment still stands on
   that value within half a row:
   - `reload_anchor_scroll::apply` (writer `anchor.initial.hold_target`)
   - `view_state_memory::restore_scroll_when_ready` (writer
     `view_state_restore`) — reached through
     `view_session::restore_browser_place`, which is what
     `route_to_place`'s second step calls.
4. Record the stand-down in `diagnostic_trail` so the next investigation
   sees the decision rather than an absence.

**Test 1a** (display test): drive a `RevealTrack` navigation from a
viewport scrolled away from the playing track. Assert on the trail via
`viewport_steps()`: after the `centered.reveal.instant` write, no
`anchor.initial.hold_target` and no `view_state_restore` write carries a
different value. Assert writer names and order — never the pixel number.

**Test 1b**: the stand-down must not become a viewport freeze. A user
scroll after the reveal, then a reload, restores the *user's* position and
not the reveal's. This is the test that fails if step 2 forgets a clearing
seam.

## Task 2 — the row height can be contradicted

Two defects, fix both:

1. `track_list_reload.rs:617` schedules the re-measurement only under
   `if is_queue`. The flat library list never plans one. Schedule it for
   every reload; the existing generation guard in
   `schedule_section_measurement_attempt` already makes a stale arming
   harmless, and its re-arm loop already handles `configure()`'s own
   `changed` emission.
2. `settled_row_height` returns `None` when the widget measurement and
   `upper / n_rows` disagree, and every caller treats that as "no
   information". It is information: `upper` is a value this code seeded
   from the remembered height, while the measurement comes from realized
   rows. When a *uniform* measurement above `ROW_MIN_HEIGHT` contradicts
   the remembered height, replace the remembered height with it — cache and
   persisted setting both.

Keep the asymmetry deliberate and comment it: a non-uniform measurement, or
one at or below the CSS floor, still says nothing. Only a clean
disagreement overrules.

Also in this task, low risk: `ui.row_height.comfortable` and
`ui.row_height.compact` are dead since `7a1e7aba11` retired the density
feature. Drop them in a migration so the next investigation does not read
them as live.

**Test 2a** (real database): seed `ui.row_height` with a value the
fixture's rows contradict, run a reload, assert the persisted value
afterwards equals the allocated one. This is the test that would have
caught the reported bug outright.

**Test 2b** (unit, `list_geometry`): a uniform measurement contradicting
the remembered height replaces it; a non-uniform one does not; one below
the CSS floor does not.

**Test 2c**: the library list schedules its re-measurement. Guard against
the `if is_queue` regression returning.

## Control arm — required before this strand is done

For each of 1a, 1b, 2a, 2b, 2c: revert the corresponding production change,
run the test, confirm it fails, paste the failure output into this file
under "Acceptance evidence", then discard the reversion. A test whose red
state was never observed is not evidence.

## Verification

- The tests above, each with its mutation probe recorded.
- The gate list from `merge-readiness` — never hand-assembled.
- Do not run the closing manual check here; it belongs to the mother plan's
  post-merge list, because it needs strand B's changes present too.

## Acceptance evidence

All display probes below ran in separate private D-Bus/Xvfb/XDG sessions with
the fake audio sink. Each temporary production regression was restored before
the next probe.

### Test 1a — reveal intent outranks later restore writers

Mutation: removed the `view_state_restore` deliberate-destination stand-down.

```text
thread 'ui::track_list::track_list_reload::reveal_track_display_tests::nav_10b_reveal_intent_outranks_later_restore_writers' panicked at crates/reprise-gnome/src/ui/track_list/reveal_track_display_tests.rs:438:17:
a later restore must not contradict the reveal; ordered trail: [Write { writer: "centered.reveal.seed", value: 4110.0 }, Observed { value: 4110.0 }, Write { writer: "centered.reveal.instant", value: 4110.0 }, ScrollTo { writer: "centered.reveal.anchor", position: 137 }, Write { writer: "view_state_restore", value: 4200.0 }, Observed { value: 4200.0 }, Write { writer: "hold", value: 4110.0 }, Observed { value: 4110.0 }, Observed { value: 6588.0 }, Write { writer: "hold", value: 4110.0 }, Observed { value: 4110.0 }]
FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2836 filtered out
mutation_probe_exit=101
```

### Test 1b — user scroll releases reveal intent

Mutation: removed the capture-phase scroll controller's call to
`clear_deliberate_destination`.

```text
thread 'ui::track_list::track_list_reload::reveal_track_display_tests::nav_10b_user_scroll_releases_the_reveal_before_a_reload' panicked at crates/reprise-gnome/src/ui/track_list/reveal_track_display_tests.rs:502:5:
assertion `left == right` failed: direct user input must take ownership from the reveal
  left: Some(4110.0)
 right: None
FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2836 filtered out
mutation_probe_exit=101
```

### Test 2a — real database replaces contradicted geometry

Three preliminary mutations stayed green and exposed fixture timing that let
an earlier measurement correct the setting before the explicit reload. Their
outputs were retained rather than counted as acceptance evidence:

```text
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2836 filtered out; finished in 0.35s
mutation_probe_exit=0

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2836 filtered out; finished in 0.32s
mutation_probe_exit=0

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2836 filtered out; finished in 0.50s
mutation_probe_exit=0
```

The fixture was then corrected to drain its initial measurement and present
stale cache, persistence, and adjustment geometry to the explicit reload.
Mutation: removed the contradiction fallback from `remember_if_settled`.

```text
thread 'ui::track_list::track_list_reload::display_tests::library_reload_replaces_a_contradicted_persisted_row_height' panicked at crates/reprise-gnome/src/ui/track_list/track_list_reload_display_tests.rs:245:5:
assertion `left == right` failed
  left: Some(53.0)
 right: Some(34.0)
FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2836 filtered out
mutation_probe_exit=101
```

### Test 2b — only a clean uniform contradiction overrules

Mutation: made `contradicting_row_height` return no contradictory height.

```text
thread 'ui::list_geometry::tests::uniform_widget_measurement_can_contradict_seeded_upper' panicked at crates/reprise-gnome/src/ui/list_geometry.rs:572:9:
assertion `left == right` failed
  left: None
 right: Some(RowHeight(34.0))
test ui::list_geometry::tests::uniform_widget_measurement_can_contradict_seeded_upper ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2836 filtered out
mutation_probe_exit=101
```

### Test 2c — library reload schedules measurement

Mutation: restored the old `if is_queue` condition around measurement
scheduling.

```text
thread 'ui::track_list::track_list_reload::display_tests::library_reload_schedules_row_height_measurement' panicked at crates/reprise-gnome/src/ui/track_list/track_list_reload_display_tests.rs:273:5:
assertion `left == right` failed: [
    "0ms QuerySet total=200 source=library sort_field=artist sort_dir=asc filter_len=0 exclude_ai=false",
    "0ms ItemsChanged position=0 removed=0 added=200",
    "10ms StackPage page=list",
    "86ms QuerySet total=200 source=library sort_field=artist sort_dir=asc filter_len=0 exclude_ai=false",
    "86ms ItemsChanged position=0 removed=200 added=200",
    "164ms StackPage page=list",
]
  left: 0
 right: 1
FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2836 filtered out
mutation_probe_exit=101
```

### Restored green controls

```text
nav_10b_reveal_intent_outranks_later_restore_writers: 1 passed; 0 failed
nav_10b_user_scroll_releases_the_reveal_before_a_reload: 1 passed; 0 failed
library_reload_replaces_a_contradicted_persisted_row_height: 1 passed; 0 failed
uniform_widget_measurement_can_contradict_seeded_upper: 1 passed; 0 failed
library_reload_schedules_row_height_measurement: 1 passed; 0 failed
v79 migration and schema tests: 3 passed; 0 failed
```

## Regression A1 — measured outcome 2026-08-25

Commit `c6683d8e87` remains unchanged. It clears reveal intent at the real
Back/Forward routing funnel, but its message incorrectly says it fixes the five
pre-existing regressions. It did not: the four Back fixtures drive the lower
`set_source` + `restore_browser_place` boundary directly, and the fifth test is
the independent delete-follow journey.

The lower boundary cannot clear on every anchor or on every
`PreserveAnchor` restore: both conditions also describe Test 1a's deliberate
reveal. The measured subject separates them. In the Back fixture,
`set_source` revealed playing track `2000`, then history asked to restore the
explicit anchor for track `191`. A `RevealTrack` place instead anchors the
playing track itself. `restore_browser_place_with_viewport` therefore releases
the intent only for an explicit anchor whose track is not the playing reveal
target. Tests 1a and 1b both remain green with this rule.

The delete-follow test exposed one additional stale subject at the same
arbitration layer: automatic advance claimed a new center while the durable
destination still belonged to the deleted track. The center policy now
releases the previous track's destination synchronously, before the same-turn
catalog reload can let it overrule the new target.

### Back-after-reveal boundary mutation

The existing Back journey now asserts that `set_source` created a deliberate
reveal destination before it calls `restore_browser_place`. Mutation: removed
the new lower-boundary clearing call.

```text
PROBE Back boundary: playing_id=2000 anchor_id=191 reveal_destination=77010
PROBE plain: value=77010 expected=37400 row_height=34 top=Some(989) wanted=191
thread 'ui::track_list::track_list_reload::navback_anchor_display_tests::nav_back_lands_on_the_anchored_row' panicked at crates/reprise-gnome/src/ui/track_list/navback_anchor_display_tests.rs:302:5:
plain: Back must land on the anchored row: actual 77010, expected 37400 (row height 34); top row is Some(989), wanted 191
FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2836 filtered out
mutation_probe_exit=101
```

Restored green arm:

```text
PROBE Back boundary: playing_id=2000 anchor_id=191 reveal_destination=77010
PROBE plain: value=37400 expected=37400 row_height=34 top=Some(191) wanted=191
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2836 filtered out
```

### Delete-follow stale-subject mutation

Mutation: removed the center policy's synchronous release of the previous
track's destination.

```text
the table did not follow playback past the deleted track: actual 3297, expected 3671.5, the deleted track's place was 3297.5
ScrollRestoreStandDown writer=anchor.initial.hold_target destination=3297.50 rejected=3671.50
Reveal track_id=113 position=111 change=automaticadvance outcome=centered
FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2836 filtered out
mutation_probe_exit=101
```

### Final prescribed harness

The unchanged harness resolved all names against this worktree's 814 ignored
tests and ran every test in its own isolated XDG/D-Bus/Xvfb/TMPDIR arm.

```text
RESULT nav_back_lands_on_the_anchored_row: pass
RESULT nav_back_lands_on_the_anchored_row_in_the_full_journey: pass
RESULT nav_back_lands_on_the_anchored_row_when_the_sort_differs: pass
RESULT nav_back_lands_on_the_anchored_row_when_the_table_had_focus: pass
RESULT nav_10b_deleting_the_running_track_keeps_the_follow_to_the_next_one: pass
BALANCE wt=/home/marvin/Projects/reprise-the-table-follows-the-music-again-a pass=5 fail=0
ARMEXIT=0
```

Final Task 1 controls:

```text
nav_10b_reveal_intent_outranks_later_restore_writers: 1 passed; 0 failed
nav_10b_user_scroll_releases_the_reveal_before_a_reload: 1 passed; 0 failed
```

### Merge-readiness wrapper

The clean-tree wrapper passed against the strand's immutable base
`1ce3dd3a15568fbe68d3557fd71f0efe735931dc`. Android source quality was routed
to its dedicated suite with `MERGE_READINESS_SKIP_ANDROID_QUALITY=1`; this
strand changes no Android path.

```text
== display test summary ==
passed: 571
failed: 0 of 571
== Runtime service bus tests ==
test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out
warning: 1 allowed warning found
Merge-readiness checks passed against 1ce3dd3a15568fbe68d3557fd71f0efe735931dc
```

At the end of the run, live `origin/dev` had advanced and the topic branch was
five commits behind it. The commits were not rebased or rewritten because this
run explicitly preserves `c6683d8e87` and every commit below it.
