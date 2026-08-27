---
slug: the-panels-yield-the-table-its-width
worktree: /home/marvin/Projects/reprise-the-panels-yield-the-table-its-width
branch: feature/the-panels-yield-the-table-its-width
phase: coded
codex_session:
created: 2026-08-27
---
# The panels yield the table its width

At a 1024 px window the navigation sidebar paints **over** the track table while
the info panel takes real space. Two panels, two different mechanisms, one of
them broken. This plan puts both under one rule and gives the table a slot it
can use.

## What was measured

Headless arms: Xvfb + openbox, `GSK_RENDERER=cairo`, isolated `XDG_*`, copy of
the real library DB (`reprise.db` + `-wal` + `-shm`), release binary built from
the dirty `dev` worktree (d5c1755 plus unrelated first_run / device_sync /
strings / po changes — none of which touch breakpoints or split views, and both
arms share the same binary anyway).

| Arm | Window | Nav sidebar opened | Result |
|---|---|---|---|
| control | 1600×900 | yes | pinned — content starts at x=240, table reflows |
| defect | 1024×768 | yes | overlay — content still starts at x=0, Title column buried |

Same binary, same profile, same click path. Only the width differs.

### Cause 1 — the window applies exactly one breakpoint

`Adw-1.gir`, `AdwWindow.add_breakpoint`: *"If multiple breakpoints can be used
for the current size, the last one is used."* Breakpoints are **exclusive, not
additive**. Three are attached to the main window, in this order:

1. `ui/window/library_shell.rs:461` — `MinWidth 800`, carries the setter
   `split_view.collapsed = false`. The only thing that pins the nav sidebar.
2. `ui/compact/compact_mode_suggestion.rs:24` — `MaxWidth 680 OR MaxHeight 480`,
   compact-mode toast, no layout setter.
3. `ui/window/responsive_side_panels.rs:143` — `MaxWidth 1400`, closes both
   panels on entry.

Between 800 and 1400 px both #1 and #3 match, #3 was added last, #3 wins, and
**#1's setter never runs**. The split keeps its constructor default
`collapsed(true)` (`library_shell.rs:299`) and behaves as an overlay across the
whole 600-px-wide band. Above 1400 px only #1 matches, the setter applies, the
sidebar pins — which is exactly the control arm.

### Cause 2 — the info panel is never collapsible

`ui/now_playing/now_playing_column.rs:44` builds its split with
`collapsed(false)` and no breakpoint anywhere sets it to `true`. It is pinned at
every width down to zero. Two panels, two mutually inconsistent mechanisms —
precisely the asymmetry the user sees.

### Why the accidental overlay is worse than it looks

From the same GIR entry: `AdwOverlaySplitView` "supports an edge swipe gesture
for showing the sidebar, and a swipe from the sidebar for hiding it. Gestures are
only supported on touchscreen, but not touchpad." There is **no scrim, no
click-outside-to-close, no Escape** on a collapsed overlay split view. On the
desktop the only way out is the toggle button. Today's state is not "libadwaita's
overlay pattern, undecorated"; it is a panel covering the work surface with no
dismissal affordance.

### Cause 3 — the table has no slot to adapt into

Persisted column widths from the real DB (`settings.ui.column_widths`, visible
layout `cover,title,artist,album,year,duration,rating`):

```
cover 40 · title 496 · artist 177 · album 184 · year 90 · duration 100 · rating 90  =  1177
```

That already overflows a 1024 px window with both panels closed. With both panels
*pinned* at 1024 the slot would be 1024 − 240 − 300 = **484 px**.

Note for anyone extending this: `column_layout.rs::column_width_policy` (Title
120, Artist 260) is **not** what is live once a user has resized a column. A fold
floor computed from those constants describes a field that does not exist.

### Cause 4 — the fold notice outlives its condition

The toast *"Some columns were folded to fit the window"* was on screen while
Artist, Album, Year and Rating were all visible. Not a fold-logic defect: during
the 1600 → 1024 resize the panels were still open, the content pane passed
through 724 px ≤ 760, the breakpoint applied and posted the toast; then
`responsive_side_panels` closed both panels, the content went back to 1024 px,
the breakpoint unapplied and restored the columns — and the toast stayed up.
`ResponsiveColumns::install_notice` posts on apply with no counterpart on
unapply.

---

## Decisions taken in the grill

| # | Decision |
|---|---|
| 1 | Below the threshold the panels are **mutually exclusive** — never an overlap, both stay reachable |
| 2 | Threshold = `SIDEBAR_MIN_WIDTH + PANEL_WIDTH + FOLD_BREAKPOINT_WIDTH` = **1300**, derived, never written as a literal |
| 3 | Crossing down closes the **info panel**; the nav sidebar stays (it carries the persisted preference) |
| 4 | `collapsed` moves to **one `AdwBreakpointBin` per split**; window breakpoints carry no layout setters ever again |
| 5 | New msgid *"The info panel was closed to fit the window"*, seven catalogs, de + es translated |
| 6 | **Undo is a deliberate override**: both panels stay open and the constraint rests until the window is wide again |
| 7 | The fold toast is dismissed when the fold ends. Concerts folding and width clamping stay out |
| 8 | Guarded by a source guard, a threshold-coherence unit test, and one display test using a margin, not an equality |

---

## Tasks

### Task 1 — take `collapsed` off the window breakpoint

`ui/window/library_shell.rs`

- Delete the `MinWidth 800` window breakpoint and its setter (lines 461–468).
- Build the split with `collapsed(false)` — the **wide** state becomes the
  default — and wrap it in an `adw::BreakpointBin` whose condition is
  `MaxWidth (SIDEBAR_BREAKPOINT_WIDTH - 1)` with the setter `collapsed = true`.
- The bin needs `set_size_request(1, 1)`. `AdwBreakpointBin` has no minimum size
  of its own; `track_list_builder.rs:83` already does exactly this for the same
  reason.
- Add the bin to `LibraryShell` as the new root and pass it at `window.rs:371`
  (`LibraryPlayerBarShell::new`) instead of `split_view`. That is the only place
  the split is mounted as a widget — every other reference
  (`responsive_side_panels`, `device_sync_feedback`, `window_navigation`) takes
  the split itself and stays valid.
- Rewrite the constructor comment. It currently explains why the split starts
  collapsed; that reason is gone.

The inversion is load-bearing, not cosmetic. A breakpoint bin restores the
captured pre-apply value on unapply, so the default must describe the wide state
and the breakpoint the narrow one — the shape the GIR's own example uses. Today's
file does the opposite, which is why a missing setter degraded to "overlay"
instead of "pinned".

A bin measures its own allocation and takes no part in the window's
one-breakpoint-at-a-time arbitration, so a fourth window breakpoint added next
year cannot disarm this again. That immunity is the point; merging the three
window breakpoints into one exclusive cascade would fix the symptom and leave the
trap armed.

### Task 2 — the same mechanism for the info panel, at the same window width

`ui/now_playing/now_playing_column.rs`

Wrap its split in an `adw::BreakpointBin` (again `set_size_request(1, 1)`) with
the setter `collapsed = true` and the condition
`MaxWidth (SIDEBAR_BREAKPOINT_WIDTH - 1)` — **the same threshold as task 1, not a
derived smaller one.**

The reasoning has to be in the comment, because the obvious alternative is
wrong. This bin sits inside the library split's content pane, so one might
subtract `SIDEBAR_MIN_WIDTH` to compensate for a pinned nav sidebar. That
subtraction is incorrect: the mapping from window width to this bin's allocation
is not injective. Below 800 the nav sidebar overlays and the content pane gets
the *full* window width, so bin widths in [560, 799] correspond both to windows
below 800 and to windows in [800, 1039]. No threshold on this bin can express
"the window is below 800" while both panels may be pinned.

Task 3 is what makes the plain threshold correct *while the info panel is open*:
below 1300 at most one panel is open, so whenever the info panel is open the nav
sidebar is closed and this bin's allocation **equals** the window width. Above
1300 both may be open and the bin sees at least 1060, far above any collapse
threshold.

The ambiguous band is **not** unreachable, though, and the plan must not pretend
otherwise. The bin's breakpoint fires on allocation regardless of `show-sidebar`:
at a 1024 px window with the nav pinned and the info panel closed, the bin sees
784 and sets `collapsed = true` on a hidden sidebar. That is harmless only
because of `pin_sidebar` below — it is what makes the band inert, not
unreachable. Anyone who later removes `pin_sidebar` on the grounds that the band
cannot be hit will reintroduce a spuriously re-opening info panel.

**Also set `pin_sidebar(true)` on both splits** (task 1's and this one). Per the
GIR, "collapsing the split view automatically hides the sidebar widget, and
uncollapsing it shows the sidebar" unless the sidebar is pinned. Without this,
crossing a collapse threshold moves `show-sidebar` behind the back of
`responsive_side_panels`, which writes that property from four handlers — and a
closed info panel would spuriously re-open on uncollapse. With `pin_sidebar`,
`responsive_side_panels` is the single owner of visibility and `collapsed` only
ever decides pinned-vs-overlay. The `connect_collapsed_notify` handlers in
`responsive_side_panels` that exist to undo the auto-show become redundant;
remove them or state why they stay.

### Task 3 — below 1300 px, at most one panel is pinned

`ui/window/responsive_side_panels.rs`

- Replace `CONSTRAINED_WIDTH = 1_400` with the derived value
  `SIDEBAR_MIN_WIDTH + PANEL_WIDTH + FOLD_BREAKPOINT_WIDTH` (240 + 300 + 760 =
  1300): the width at which two pinned panels push the table below its own fold
  threshold. This requires raising the visibility of `SIDEBAR_MIN_WIDTH`
  (`ui/sidebar/sidebar_presentation.rs:19`) and `FOLD_BREAKPOINT_WIDTH`
  (`ui/track_list/responsive_columns.rs:14`) to `pub(in crate::ui)`;
  `PANEL_WIDTH` already is. `scripts/check-architecture.sh` enforces file sizes
  and `mod.rs` surfaces only — there is no module ban to violate here.
- On entering the constrained band, close the **info panel** only. The nav
  sidebar keeps its state; it carries a persisted preference
  (`get_sidebar_collapsed`), while the info panel is already driven transiently
  through `set_transient_visibility`. `constrained_visibility` becomes
  "info panel off, library untouched" instead of "both off".
- While the constraint is active and has not been overridden, opening one panel
  closes the other. This belongs inside the existing
  `connect_show_sidebar_notify` handlers, which already own the `applying`
  re-entrancy flag — not a new mechanism.
- **Undo overrides.** `note_user_change`/`changed_by_user` already means "stop
  enforcing"; the Undo button restores the full pre-snap snapshot, both panels
  open, and the constraint rests until the window goes back above the threshold.
  Mutual exclusion must not corrupt that snapshot — it is taken on `apply`,
  before anything closes.
- No toast for a user-driven exclusion. The user just clicked to open the other
  panel; announcing the consequence of their own click is noise. Only the
  threshold crossing announces.
- **Announce nothing when nothing closed.** `ConstraintState::apply` currently
  returns `Some(target)` whenever `current.any_open()`. With
  `constrained_visibility` reduced to "info off, library untouched", a user at
  1024 with only the nav open who narrows further gets a target identical to the
  current state — and still a toast announcing a close that did not happen. Guard
  it: return `None` when the computed target equals the current visibility. This
  is the same class of non-event the existing `announces_collapse` gate was
  already reaching for.
- **Three existing tests encode the old two-panel semantics and all three need
  rewriting to the new rule, not deleting:**
  `style_7_constrained_window_closes_both_side_panels_as_one_transition`
  (asserts both go off), `style_7_undo_restores_the_exact_pre_snap_panel_state`
  and `style_7_widening_restores_pre_snap_state_unless_the_user_changed_it`
  (both build their snapshots on the two-panel transition). The latter two are
  the dangerous ones — they can keep passing while asserting nothing that still
  matters.
- `style_7_default_window_is_not_born_below_its_own_breakpoint` compares against
  `SessionState::default().window_width` = 1440 (`session.rs:29`). It stays green
  at 1300 with more headroom than before; keep it.

### Task 4 — the toast

`ui/strings.rs` and `po/`

- Replace `SIDE_PANELS_CLOSED` ("Side panels were closed to fit the window") with
  a msgid reading **"The info panel was closed to fit the window"**.
- `scripts/tests/gettext-catalogs.sh` regenerates the `.pot` from source and runs
  `msgcmp --use-fuzzy --use-untranslated` against all seven catalogs, and rejects
  any fuzzy entry. So the new msgid must be present in ar, bn, de, es, fr, hi and
  zh_CN. `de` and `es` are in `complete_locales` and must be translated; the other
  five may carry an untranslated entry. No fuzzy markers anywhere.

### Task 5 — dismiss the fold notice when the fold ends

`ui/track_list/responsive_columns.rs`

`install_notice` posts a toast on apply and has no counterpart on unapply. Keep
the `adw::Toast` handle in an `Rc<RefCell<Option<adw::Toast>>>` and `dismiss()`
it from `connect_unapply` — the same pattern `responsive_side_panels` already
uses with its `active_toast` cell.

### Task 6 — regression tests

1. **The source guard — this is the test that watches the trap.** A plain unit
   test that reads the window sources with `include_str!` and asserts that no
   `add_breakpoint` call on the *window* is accompanied by a `collapsed` setter.
   The repo already uses this idiom
   (`library_shell.rs::browse_1_music_builds_only_the_canonical_track_surface`).
   It survives a widget refactor, which the display test does not.

2. **Threshold coherence.** A unit test asserting both bins use the same
   condition width, and that `CONSTRAINED_WIDTH` equals
   `SIDEBAR_MIN_WIDTH + PANEL_WIDTH + FOLD_BREAKPOINT_WIDTH`. Prevents the two
   panels from silently drifting into different modes.

3. **One display test, with a margin.** In a 1024×768 window with the nav
   sidebar shown, assert `!split_view.is_collapsed()` **and** that the content
   pane's x-origin in window coordinates is greater than 100 —
   `compute_bounds` against the window, the way
   `now_playing_column.rs::style_5_info_panel_surrenders_height_before_the_player_bar`
   already does. The defect yields 0, the fix ~240; a 100 px margin is nowhere
   near the pixel-rounding flake class that already bites
   `nav_10a_centering_lands_exactly_on_the_target`. Mark it
   `#[ignore = "requires a display; run via xvfb-run"]`.

An assertion on `is_collapsed()` alone would have gone green against the broken
build — the split *was* collapsed; that was the bug. The coordinate margin is
what separates the arms.

---

## Constraints for the implementation

- **`window.rs` is at 583 lines and `scripts/check-architecture.sh` fails at
  600.** Task 1 touches it. Keep the change to the single argument at line 371;
  do not add helper code there. Every `.rs` file has a hard 800-line ceiling.
- Codex's sandbox cannot write to `~/.cache`, cannot bind TCP sockets and cannot
  create a `cargo audit` lockfile. Set `XDG_CACHE_HOME=<worktree>/.codex-cache`
  and name those three failure classes in the prompt as "skip and note", or a
  "every error means stop" rule aborts in the wrong place.
- `scripts/check-display-tests.sh --rule-named` exits 0 even when tests are red.
  Grep for `FAILED`/`panicked`; never read its exit code.

## Verification

Local gate: `cargo test -p reprise-gnome`, `cargo clippy`,
`scripts/tests/gettext-catalogs.sh`, `scripts/check-architecture.sh`, and
`scripts/check-display-tests.sh` for the new display test.

Then the two-arm screenshot run, because a green suite is not evidence of a
visible layout. Harness as above; kill with `pkill -x reprise`, never `pkill -f`
(the pattern matches the invoking shell).

| Arm | Window | Action | Expected |
|---|---|---|---|
| control | 1600×900 | open both | unchanged from today: nav 0–240, content 240–1296, info 1296–1600 |
| fix | 1024×768 | open nav | content shifts to x=240, table reflows, nothing overlaps |
| fix | 1024×768 | then open info | nav closes, content 0–724, info 724–1024 |
| override | 1024×768 | cross down with both open, press Undo | both open, table 484 and scrolling, constraint resting |
| narrow | 700×768 | open nav | nav overlays; info panel is closed, not pinned beside it |

## Out of scope

- **Column folding for Concerts, Releases and Radio.** Only the track list wires
  `ResponsiveColumns`. At 1600 px with both panels open the Concerts Tickets
  column is already clipped. Real, separate.
- **Clamping persisted column widths.** 1177 px of stored widths means the table
  still scrolls horizontally at 1024 even with one panel. Clamping would
  overwrite a setting the user deliberately made; who owns the width is its own
  decision.

## Parallelität

The cut was attempted and is **rejected**. One strand, no suffix files.

Geometrically it exists — tasks 1–3 own `ui/window/**` and `ui/now_playing/**`,
task 4 owns `ui/strings.rs` and `po/**`, task 5 owns
`ui/track_list/responsive_columns.rs`. But:

- Task 5 is a toast handle and a `dismiss()` call, roughly ten lines. Its own
  worktree, Codex run, review round and landing cost more wall-clock than writing
  it inline.
- Task 4's string is consumed by task 3's changed toast call site. Splitting them
  puts a compile dependency across a branch boundary.
- Tasks 1, 2 and 3 are one mechanism. Task 2's threshold is only correct
  *because* task 3 makes the ambiguous band unreachable — that argument is
  written down in task 2 and cannot be verified by a branch that does not own
  task 3.
- Task 6's display test reads the shell that tasks 1–3 change and asserts the
  behaviour task 3 defines. Under the ownership rule it could not go green in any
  strand that does not own all three.
