---
slug: ptr-e2e-compact-flow-respec
worktree: /home/marvin/Projects/reprise-ptr-e2e-harness-debt
branch: feature/ptr-e2e-harness-debt
phase: coded
codex_session:
created: 2026-08-12
implemented: 2026-08-12
---
# Respec flows 5 and 6 against the current interface

**Goal:** The Compact portion of the pointer E2E suite described an interface
that no longer exists and, in one case, one that was never implemented. Keep
only checks for behavior the product currently exposes and name removed checks
as product gaps instead of permanent harness failures.

**Base:** This work starts from `feature/ptr-e2e-harness-debt` at
`ac79a9a597`. The five commits immediately below that revision made flows 4b,
5, and 6 reachable; before them, a stale SQL query aborted every run in flow 4.

## Measured baseline

The measurements came from run 5 on 2026-08-12 with
`PTR_E2E_PROFILE=debug`, a 1600x900 Xvfb screen, and animations disabled. That
run reached flows 1 through 6 and reported 22 failed and 32 passed checks.

### The Compact card is a 430x76 strip

`compact_player_layouts.rs` defines `MINI_WIDTH = 430`, `MINI_HEIGHT = 76`,
and `CARD_MARGIN = 0`. The capture showed a 52 px cover, metadata, waveform,
and 38 px circular Play button in a card at the top-left of the screen.

The old harness assumed a 580x360 window, metadata at `(300, 80)`, and a menu
button at `width - 105`. Those points were outside the real card. Before
`7bd0fa840f`, wheel input then reached Openbox's root window and changed the
virtual desktop, making the app appear to disappear.

### Cover, Pill, and Card have no product switching path

`CompactLayout { Cover, Pill, Card }` remains in Core settings, but GNOME has
no renderer that branches on those variants and no production UI that writes
them. The frontend always builds `build_mini()` with `CompactLayout::Card`.
The former expected log line, `compact layout changed`, does not exist; the
only relevant transition log is `window view mode changed mode=... layout=...`.

The former Cover, Pill, and Card menu-selection checks and their three geometry
checks therefore described an unimplemented feature and have been removed.

### Compact is entered through the primary menu

`primary_menu.rs` puts Compact Mode first in the View section. There is no
Compact button in the Library header. Since `8d35074760`, flow 5 correctly
enters through F10 and Return.

### Library restore has three current routes

The mini-card context menu exposes Restore Full Window as `compact.restore`.
Ctrl+M invokes the same toggle, and double-clicking the cover or title also
restores the Library window.

## Implemented packages

### Package A: current mini-card flow

- Removed `select_compact_layout`, all `compact layout changed` waits, and the
  Cover/Pill/Card geometry checks.
- Replaced stale Compact constants with the real 430x76 bounds and stable cover
  and Play edge offsets.
- Derived Play, metadata/waveform, and cover coordinates from `window_rect`.
  Every pointer action passes through `assert_point_in_window`.
- Moved the one-step volume check to the derived metadata midpoint. Static
  product inspection confirms that the card installs a discrete vertical
  scroll controller with a 0.05 volume step, so this behavior remains a valid
  runtime check.
- Right-clicks the card midpoint, captures the current context menu, and
  invokes Restore Full Window. The before/after menu comparison now starts
  from a point inside the 430x76 card.
- Retains Ctrl+M and cover double-click restore coverage.

### Package B: primary-menu navigation

The popover focuses its first entry when opened. `primary_menu.rs` orders the
focusable entries as:

1. Compact Mode
2. Edit Column Layout
3. Library Doctor
4. Import Playlist
5. Preferences

Flow 6 relies on the popover's initial Compact Mode focus, deliberately sends
no Home press, sends four Down presses, captures the focused Preferences item,
and only then presses Return. Home is not a no-op in this popover: pressing it
costs one navigation step.

### Package C: queue identity

Flow 3 queues `sine_01` and `sine_02`, then drags the first below the second.
Flow 4 independently clears that residue and establishes its own manual order
through the private scratch-bus control surface. It queries ids by fixture title
rather than guessing with table offsets, adds `sine_02` then `sine_01`, and
requires `up_next_len=2` before the first MPRIS Next. Its expected sequence is
therefore context `sine_03`, manual `sine_02`, manual `sine_01`, then context
`sine_04`, regardless of what an earlier flow left behind.

### Package D: current invalid-Year behavior

Typing `0` into Year does not create an effective dirty-session change.
Consequently Save remains disabled, Ctrl+Return never reaches `do_save`, the
save-time invalid-number log is not emitted, and no database write occurs.

The harness now records the selected track's year and the tag-write job count,
tries Return followed by Ctrl+Return, proves both database values are
unchanged, and confirms that the dialog remains until explicit Escape by
comparing the open-dialog and post-Escape captures.

### Run 6 review follow-up

Run 6 reached the end with 34 passed and 9 failed checks. The measured failures
identified six harness defects, all repaired without changing product code:

- screenshot AE metrics now use a float-safe `awk` threshold, so anti-aliased
  values such as `23166.7` no longer invert a visible-change verdict;
- Preferences opens the primary menu with F10, while the documented header
  offsets now place Main menu at 227 px and Search at 186 px from the right;
  the removed Information control has no retained constant;
- flow 4 clears inherited queue state, adds its own two manual tracks, and
  observes `up_next_len=2` before consuming either one;
- Compact playback is first established as Playing, and the derived button is
  judged by the resulting MPRIS `PlaybackStatus=Paused`, not by requiring a new
  transition log from a fixture that may already be idle;
- cleanup reports the effective process exit status after failed checks; and
- the duplicated Compact-route comment was removed.

### Run 7 review follow-up

Run 7 reached flow 6 with 30 passed checks and printed four `FAIL:` lines, but
the closing balance reported only three. Four harness findings were repaired:

- `log_fail` now appends one record to a scratch failure ledger beside
  `app.log`; cleanup derives its balance from that file, preserves the emitted
  harness output as `run.log`, and prints a distinct `TALLY MISMATCH` diagnostic
  if the ledger count ever differs from the `FAIL:` count in that run log;
- flow 5 seeds its own one-track playback context with `PlayTrackIds`, follows
  with idempotent MPRIS `Play`, waits for `PlaybackStatus=Playing`, and unwraps
  the gdbus variant so failures say `got Stopped` rather than
  `got (<'Stopped'>,)`;
- leaving Compact is recorded with its resulting Library geometry. Flow 6
  clears and reapplies Openbox's maximized flags, waits up to nine seconds for
  at least 1500x850, and skips all coordinate actions after one hard failure if
  that precondition cannot be established; and
- the app log showed the correct flow-4 order: context `track_id=3`, manual
  `track_id=4`, manual `track_id=5`, then resumed context `track_id=2`. The
  first explicit Next reached both short manual fixtures through gapless
  hand-off, so the assertion now checks their dequeue/start events as one
  ordered sequence and uses the following Next for context B. This is expected
  product behavior, not a suspected product bug.

### Run 8 review follow-up

Run 8 stopped inside flow 4 with 24 passed checks and one recorded failure;
flows 4b, 5, and 6 never started. Three further harness defects were repaired:

- ordered log assertions now record a failed check and return success like the
  other assertion helpers, so a plain call under `set -e` cannot abort the
  remaining flows; every other helper added in the run-6 and run-7 rounds was
  audited, and its non-zero result is used only behind explicit control flow;
- every route declares its expected flow count and records each flow start. The
  closing balance always states both counts, and fewer started flows force a
  failing exit even when the failure ledger and emitted `FAIL:` lines agree;
- flow 4 no longer requires an intermediate `up_next_len=1`. It proves that
  manual X starts before manual Y and independently proves that Up Next reaches
  zero, matching the measured `2 → 0` queue publication without weakening the
  queued playback order.

### Run 9 review follow-up

Run 9 completed all nine flows with 38 passed and two failed checks. The flow
coverage line and failure ledger agreed, making this the first branch run whose
complete counts are trustworthy. Both failures had one harness cause:

- the primary popover already focuses Compact Mode when F10 opens it. The
  extra Home press consumed one navigation step, so four Down presses stopped
  on Import Playlist instead of Preferences. Flow 6 now omits Home and retains
  the four Down presses;
- the focused-menu screenshot remains immediately before Return, but its check
  now compares it against a capture taken before F10. This proves that the
  primary menu visibly opened instead of merely proving the capture was not
  blank. The float-safe screenshot helper from the run-6 repair supplies the
  comparison.

### Run 10 review follow-up

Run 10 completed all nine flows with 38 passed and the same two failed checks.
Its focused-menu capture confirmed that the run-9 navigation repair selected
Preferences correctly. Both remaining failures came from the harness retaining
the pre-`AdwDialog` surface model:

- the product has logged `preferences dialog presented` since Preferences
  moved into the main window, while flow 6 still waited for `preferences
  window presented`. The wait now matches the emitted message;
- an `AdwDialog` does not create a second X11 toplevel, so searching for an
  active transient could never succeed. The flow now pairs the presentation
  log with an `assert_screenshots_differ` comparison between the focused menu
  immediately before Return and the presented dialog immediately afterward;
- every retained page and control check now uses the authored 760x680 dialog
  rectangle centered within the maximized Library window. The previous
  transient-window origin and width queries were removed, but no downstream
  page, persistence, or reachability check was dropped.

## Named product gaps

These gaps are intentionally recorded rather than represented as red harness
checks:

- **Compact layout choice is persistence without behavior.** Cover, Pill, and
  Card exist in settings, but there is no variant-specific renderer or UI
  writer. Reintroduce layout-selection checks only when both paths exist.
- **Invalid Year feedback is misleading.** `parse_number_field("0")` is an
  error, but the invalid text never makes the session dirty. Save therefore
  remains disabled with the tooltip "No effective changes" instead of
  explaining the invalid input. This branch records the UX defect and does not
  modify product code.

## Acceptance and verification boundary

Static acceptance for this implementation is:

- no Compact layout-switching assertion remains;
- every pointer point in `compact-flow.sh` uses `assert_point_in_window`;
- menu Down counts are documented with the traversed entries;
- Flow 4 contains no `OFFSET` identity guess;
- Flow 4 establishes and observes its own two-item manual queue;
- Flow 4 asserts both short manual starts under one ordered event marker,
  confirms that Up Next ends empty without requiring an intermediate length,
  and only then issues the single Next that resumes context B;
- Compact play/pause checks MPRIS status rather than a transition log;
- failure accounting is file-backed and cross-checked against preserved
  `run.log` output, and the closing balance states started and expected flows;
- fewer started flows than the selected route expects force a failing exit;
- flow 6 performs no coordinate action unless maximized geometry reaches at
  least 1500x850;
- flow 6 waits for `preferences dialog presented`, proves the hosted dialog
  visibly replaced the focused primary menu, contains no Preferences transient
  lookup, and derives every retained control coordinate from the centered
  760x680 dialog rectangle;
- the display-free harness accounting self-test passes;
- every touched shell script passes `bash -n`.

The full ptr-e2e run is deliberately not part of this implementation session.
It needs Xvfb, Openbox, and a built debug binary and is run separately by the
caller. That run must still confirm that the suite reaches its final balance
line, reports `9 of 9 flows ran`, and reports the same failure count as
`grep -c 'FAIL:'`.

## Follow-up instrumentation cleanup

`e42bfab919 chore(probe): trace rating clicks and dialog lifecycle` is temporary
instrumentation in Rust product files. Reverting it is outside this task's
scripts-and-plans-only scope and must happen in a separately authorized change
after the caller's runtime verification. The durable harness changes above do
not depend on those lifecycle probes.
