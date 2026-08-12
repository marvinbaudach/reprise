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

Flow 6 now starts from Home, sends four Down presses, captures the focused
Preferences item, and only then presses Return.

### Package C: queue identity

Flow 3 queues `sine_01` and `sine_02`, then drags the first below the second.
Flow 4 now queries ids by those fixture titles rather than guessing with table
offsets. Its expected sequence is context `sine_03`, manual `sine_02`, manual
`sine_01`, then context `sine_04`. The Up Next count assertions now share that
same observed queue identity.

### Package D: current invalid-Year behavior

Typing `0` into Year does not create an effective dirty-session change.
Consequently Save remains disabled, Ctrl+Return never reaches `do_save`, the
save-time invalid-number log is not emitted, and no database write occurs.

The harness now records the selected track's year and the tag-write job count,
tries Return followed by Ctrl+Return, proves both database values are
unchanged, and confirms that the dialog remains until explicit Escape by
comparing the open-dialog and post-Escape captures.

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
- every touched shell script passes `bash -n`.

The full ptr-e2e run is deliberately not part of this implementation session.
It needs Xvfb, Openbox, and a built debug binary and is run separately by the
caller. That run must still confirm that the suite reaches its final balance
line and that the reported failure count equals `grep -c 'FAIL:'`.

## Follow-up instrumentation cleanup

`e42bfab919 chore(probe): trace rating clicks and dialog lifecycle` is temporary
instrumentation in Rust product files. Reverting it is outside this task's
scripts-and-plans-only scope and must happen in a separately authorized change
after the caller's runtime verification. The durable harness changes above do
not depend on those lifecycle probes.
