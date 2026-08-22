---
slug: issue-backlog-wave-1-1
worktree: /home/marvin/Projects/reprise-issue-backlog-wave-1-1
branch: feature/issue-backlog-wave-1-1
phase: planned
codex_session:
created: 2026-08-22
---
# Strand 1 — four small defects: #254, #250, #405, #406

Mother plan: `docs/plans/issue-backlog-wave-1.md`. Base `origin/dev` = `1515487599`.

This strand owns, and writes **only**:

```
crates/reprise-gnome/src/ui/stats/**
crates/reprise-gnome/src/ui/playback/**
crates/reprise-gnome/src/ui/session_restore.rs
crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs
crates/reprise-gnome/src/ui/track_list/rating.rs
scripts/cua-e2e/README.md
docs/plans/issue-backlog-wave-1-1.md
```

`crates/reprise-gnome/src/ui/strings.rs` belongs to strand 2 — task 3 below needs
no new string, it reuses one that already exists.

---

## Task 1 — #254: the stats tween test samples a race, not a behaviour

**Where.** `crates/reprise-gnome/src/ui/stats/stats_view_entrance_tests.rs:122-238`,
`stats_19_period_switch_tweens_bars_without_restarting_static_content`.

**What is actually wrong.** The test captures an intermediate width through
`test_settle::settle_until` (line 148), waits, and then asserts the width grew
(`stats_view_entrance_tests.rs:207`). The settle predicate only waits for the
tween to have *started* — a width greater than zero. On a loaded machine the
tween can be **finished** by the time that predicate first passes, and then
`growing == final` and the assertion fails. It has been reproduced 5 of 5 under
load at `--test-threads=1`, and it has blocked two unrelated PRs (#247, #251).

The tween's duration is a known constant: `STATS_TWEEN_MS = 250` in
`crates/reprise-gnome/src/ui/motion.rs:29,77-78`.

**The fix.** Stop asserting on catching the tween mid-flight. Take one of these
two shapes and take it for the genre segment **and** the bar, so the file does not
keep two different disciplines for the same problem:

- make the settle predicate require the sample to be *strictly between* zero and
  the target — `width() > 0 && width() < target` — so a finished tween never
  satisfies it and the test waits for a genuinely intermediate frame; **or**
- drop the intermediate comparison entirely and assert only that the final width
  reaches the new target, leaving "it animates at all" to its own test.

Prefer the first if the target width is available to the test without new
production API. If it is not, take the second and add a separate, small test
that the animation is armed at all (for example that the reveal's fraction is
below 1.0 immediately after the switch) rather than deleting the coverage.

**Do not** widen a timeout, add a sleep, or mark the test `#[ignore]`. The defect
is in the oracle, not in the schedule.

**Acceptance.** The test must fail for the right reason and pass for the right
reason:

1. The changed test still fails when the production tween is broken. Prove it by
   a mutation: force the genre segment's reveal fraction to its final value
   immediately (in `stats_entrance.rs`, the `motion::stats_timed`/`animate_group_at`
   call site), run the test, record that it goes **red**, then revert the mutation
   and record that it goes green again. Both runs and the exact mutation belong in
   this file under `## Result`. A test that passes both with and without the
   mutation has not been fixed, it has been disarmed.
2. It passes under load. Run it with the machine busy, `--test-threads=1`, five
   consecutive times, and record 5/5.

The test needs a display; run it through `scripts/check-display-tests.sh` or
`xvfb-run` per this repository's usual route.

---

## Task 2 — #250: `REPRISE_SMOKE_REPEAT=all` is overwritten by the session restore

**Where, and why it is not a race.** The issue reads like a 62 ms race. It is
not; it is a deterministic ordering inside one synchronous startup:

- `arm_smoke_repeat` (`crates/reprise-gnome/src/ui/playback/player_controller_wiring.rs:291-308`)
  sets `Repeat::All` on the controller's queue. Its only caller is
  `crates/reprise-gnome/src/ui/playback/player_controller.rs:500`, in the
  constructor.
- `restore_session_queue` (`crates/reprise-gnome/src/ui/playback/session_player.rs:51-99`)
  builds a fresh `Queue::new()` at line 78, calls `restore_snapshot`, and installs
  it at line 92. `Queue::restore_snapshot`
  (`crates/reprise-core/src/queue/snapshot.rs:61`) assigns `self.repeat = snapshot.repeat`
  unconditionally.
- `restore_session_queue` is reached from `session_restore.rs:62-71`
  (`restore_runtime`), called from `window/window_runtime_wiring.rs:649` — later in
  the same synchronous sequence than the constructor.

So the persisted session's repeat always wins, and the hook is a guaranteed no-op
whenever a session snapshot exists. Any headless E2E that relies on it runs with
repeat **off** and passes vacuously.

The contrasting path that gets this right is `reset_to_stopped`
(`crates/reprise-gnome/src/ui/playback/player_event_handling.rs:351-358`): it
reads the live `repeat` and `shuffled`, rebuilds the queue, and writes them back.

**The fix.** Make the smoke override outrank the restore, once, in one place.
Two acceptable shapes:

- call the arming **after** `restore_runtime` instead of in the constructor
  (`window/window_runtime_wiring.rs:649` is the seam), or
- give the restore the same courtesy `reset_to_stopped` already has: read the
  armed override before installing the restored queue and re-apply it afterwards.

Whichever is chosen, there must be exactly **one** place that decides the repeat
mode after start-up. Do not leave the constructor arming in place *and* add a
second application — that is two truths fighting over one field, which is the
defect this issue reports.

The env var must keep its current spelling and its current accepted values; this
is a verification hook other harnesses already call.

**Acceptance — a state assertion, not a log line.** The issue is explicit that a
silent no-op in a verification hook is worse than a missing hook, so the proof
may not be the existing `INFO` line.

1. **Displayless unit test.** Extract the decision — "given the snapshot's repeat
   and the armed override, which repeat does the app end up in" — as a pure
   function in `crates/reprise-gnome/src/ui/playback/`, and test it directly:
   override present ⇒ `All` regardless of the snapshot; override absent ⇒ the
   snapshot's value. This is the part that must never regress silently.
2. **Observed end state.** Add a test that runs the real ordering (constructor,
   then restore) with the override armed and a snapshot carrying `Repeat::Off`,
   and reads the controller's queue repeat afterwards. If that cannot be reached
   without a display, make it a display test rather than dropping it — do not
   substitute a test of the pure function for this one, they prove different
   things.
3. **Mutation proof.** Revert the fix by hand, confirm the new test goes red,
   restore it, confirm green. Record both in `## Result`.
4. **The workaround note.** `scripts/cua-e2e/README.md:155-157` records that
   `play-11-stop-repeat-all` clicks the transport button *because* this hook is
   overwritten. Update that note to say the hook now holds, and reference this
   issue. **Do not change the E2E case itself** — switching it back is a separate
   decision with its own run, and this strand does not own it.

---

## Task 3 — #405: rating stars share one accessible name

**Where.** `crates/reprise-gnome/src/ui/track_list/rating.rs:265-286`, `build_star`.
Each star is a `gtk4::Button` whose child is a `gtk4::Label` carrying `★`/`☆`.
Line 277 installs a per-star tooltip through `lazy_tooltip::install(&button, strings::rate_n_stars(star))`.

**Why the tooltip is not enough.** `crates/reprise-gnome/src/ui/lazy_tooltip.rs:19-33`
serves the text only from `query-tooltip`; it never writes an AT-SPI property.
The accessible name therefore falls back to the label's glyph, which is identical
on every star — so assistive technology cannot say which star it is on and
"set three stars" is not addressable.

**The fix.** Give each star button an explicit accessible label from the string
that already exists:

```rust
button.update_property(&[gtk4::accessible::Property::Label(&name)]);
```

`strings::rate_n_stars(n)` (`crates/reprise-gnome/src/ui/strings.rs:424-433`) is
already per-star, already translated, and already the tooltip text — reuse it,
do not invent a second wording, and do not add a string (strand 2 owns that file).
Follow the idiom at `crates/reprise-gnome/src/ui/source_add_action.rs:61-63`,
which sets tooltip and accessible label from the same value.

Where the tooltip carries something the label does not, add
`Property::Description` as well; if it carries nothing extra, do not add an empty
one.

The stars stay `set_focusable(false)` (`rating.rs:275`). The row is the
collection's sole tab stop and rating stays keyboard-reachable through Edit Tags.
This task is about **naming**, not about adding a tab stop — do not change the
focus model.

**Acceptance, and the gap that has to be stated rather than papered over.**
gtk4-rs 0.11.4 exposes `update_property` but **no getter** for accessible
properties (`AccessibleExtManual` in the bound crate has `update_property`,
`update_relation`, `update_state` and nothing that reads them back). A test
therefore cannot read the label off the widget.

So:

1. Assert what *can* be asserted: a displayless test that
   `strings::rate_n_stars(1..=5)` yields five distinct strings, so the names the
   widget is given are distinguishable in the first place.
2. Make the wiring itself checkable statically. `scripts/check-accessibility-semantics.sh`
   already enforces marker comments and per-role property requirements over
   `crates/reprise-gnome/src/ui`. Extend it so that a star button — or, stated
   generally, this rating widget's buttons — is required to carry
   `Property::Label`, and confirm the gate goes **red** when the new
   `update_property` call is removed and green when it is restored. Record both
   runs. Keep the extension narrow: it must not start failing on unrelated files.
3. Write into `## Result`, in one short paragraph, that no widget-level readback
   exists in this GTK binding and that the AT-SPI tree itself was therefore not
   asserted by this strand. Say it plainly. Do not add a test that cannot fail in
   order to make the section look complete.

---

## Task 4 — #406: the hover preview ignores where the pointer entered

Same file as task 3, and measured on 2026-08-22 out of the evidence that filed the
issue (`~/.cache/reprise-explore-evidence/2026-08-11-m4b/hover-affordance-sweep-seed-{11,29}/trajectory.jsonl`).

**What the sweep actually found.** All 14 occurrences — 7 per seed, and the two
seeds walked an identical path, so this is one path run twice — are the *same*
element: an outline star (`☆`) in a track row's rating cell, always hovered
immediately after the filled star (`★`) next to it. Every one reports
`changed_pixels: 0, max_channel_delta: 0` inside the element's own AT-SPI frame —
a hard zero, not a near miss against the oracle's `0.02` ratio /
`6` channel-delta thresholds.

**The mechanism.** In `crates/reprise-gnome/src/ui/track_list/rating.rs`:

- `connect_enter` (lines 231-240) receives the entry coordinates and **discards
  them**, doing `preview.set(0)` unconditionally.
- `connect_motion` (lines 241-252) is the only place that computes `star_at_x`.

The harness parks the pointer and warps it onto the target's centre, so an `enter`
arrives with no accompanying `motion`. For a star at or beyond the current
rating — already outline before the hover — `preview = 0` renders exactly what was
already on screen, hence zero changed pixels. The filled star one step earlier does
change, which is why `★` never appears in the failing list and `☆` always does.

The stars are flat buttons (`set_has_frame(false)`, `rating.rs:272`) with no
`:hover` rule on `.reprise-rating-star` (`rating.rs:76,138-145`) and no
`.reprise-hover` class, so there is no theme fallback to cover for this. The
widget's hover affordance *is* the preview re-render in `refresh()`
(`rating.rs:329-361`) — and at the instant of entry it is wrong.

**The fix.** Make entry and motion agree: compute the previewed star from the
coordinates `connect_enter` already receives, through the **same** function
`connect_motion` uses. Extract that mapping if it is not already a named function,
and have both handlers call it. Do not special-case the harness, and do not add a
CSS `:hover` rule as a substitute — that would paint over a control-flow defect
with a second, unrelated affordance.

**Acceptance.**

1. **Displayless test of the mapping.** For a widget of known width and a known
   current rating, entering within star *n*'s horizontal band previews *n* — for
   every *n*, including the outline stars beyond the current rating, which are the
   ones the sweep caught. The test must fail if `enter` goes back to `set(0)`.
2. **Mutation proof.** Restore the unconditional `preview.set(0)` in
   `connect_enter`, confirm the new test goes **red**, revert, confirm green.
   Record both.
3. **Leave the sweep alone.** Re-running the CUA hover sweep is the harness-level
   proof and it belongs to the post-merge cross-checks in the mother plan, not to
   this strand. Note in `## Result` that this strand proves the mechanism and not
   the sweep's numbers.
4. Say plainly in `## Result` what the caveat is: under a real pointer gliding
   into the cell, `motion` follows `enter` within milliseconds and hides the
   defect, so this is a defect at the instant of entry — reliably visible to
   automation and fast pointer jumps, not guaranteed to be visible on every human
   hover. That is a reason to fix it, not a reason to claim more than was measured.

---

## Result

*(Codex fills this in: for each task the commands run, the counts, the mutation
proofs with their red and green runs, and anything that could not be proven and
why.)*
