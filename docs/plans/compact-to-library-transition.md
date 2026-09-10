---
slug: compact-to-library-transition
worktree: /home/marvin/Projects/reprise-compact-to-library-transition
branch: feature/compact-to-library-transition
phase: refactored
codex_session:
created: 2026-09-09
---
# The mini-player becomes its own window

## Why

Switching compact → library looks slow and broken: the full app appears in a
small corner and only then spreads over the screen.

Measured (`compact-to-library-transition.findings.md`; release build, real
1959-track library, real Wayland/GNOME, 2 runs / 6 toggles):

- `content_host.set_content(&self.full_root)` blocks the GTK main thread
  **46–92 ms** — it reparents the whole library tree into the `AdwToolbarView`.
- A further **~100 ms** passes before the first correct-size frame. That gap is
  **constant and independent of the remount cost**, so the two costs are
  additive, not causal.
- In that window exactly **one** frame is presented: toplevel still 430×76 with
  the library content already in it.

Refuted by measurement, do not re-investigate: no double configure
(`set_default_size` + `maximize` coalesce), and not "many squeezed frames"
(exactly one, always).

### What the grill changed

Two facts turned the original approach around.

**1. The current ordering is deliberate.** Commit `73e29ba70b` ("fix: stabilize
compact view restoration", 2026-07-14) introduced it precisely so that "beim
Verlassen der Compact-Ansicht kein sichtbar aufgeblasener Compact-Inhalt als
Zwischenzustand erscheint". Simply reversing it trades the user's complaint for
the one that commit was written to fix. Its own design note conceded that "das
tatsächliche Frame-Pacing unter GNOME/Wayland bleibt ein manueller Sichtcheck" —
it was never measured.

**2. A large part of the symptom is not ours to fix in-frame.**
`org.gnome.desktop.interface enable-animations` is `true`, so Mutter *itself*
animates the 430×76 → maximized state change, zooming the window up from the
corner. No app-side frame ordering removes that; the compositor draws it. (Stated
as the strongest available explanation, not as proof — verifying it directly
would mean flipping a global desktop setting from a session, which this project
does not do.)

The user's requirement is that the app "springt sofort in die fullview größe und
ist auch gleich richtig positioniert wie vorher" — explicitly *not* "grow in
place, then move".

### The decision

**The mini-player gets its own toplevel. The library window is only hidden and
shown — never resized, never unmaximized, never moved.**

This dissolves the whole problem class instead of trading artefacts:

| | one window (today & first draft) | two windows |
|---|---|---|
| library remount per toggle | 46–92 ms | none |
| resize round-trip | ~100 ms | none |
| wrong-size frame | 1, unavoidable | none |
| Mutter maximize animation | always | none |
| position after restore | compositor's choice | unchanged, it never moved |

Tasks 1 and 2 of the draft (a `GtkStack` page swap, and a same-frame swap
trigger) are **dropped**: with no resize and no reparenting, neither has anything
left to solve. The crossfade idea is dropped too — the requirement is an
instant jump, not an animation.

This reverses the "Ein-Fenster-Pfad" of 2026-07-14 knowingly. That decision was
made to avoid an intermediate visual state; two windows remove the intermediate
state altogether.

## Scope

`crates/reprise-gnome/src/ui/compact/*`, `ui/window/window.rs`,
`ui/window/window_decorations.rs`, `ui/session_restore.rs`.

Out of scope: general app slowness (separate investigation; this branch is also
missing `#849`/`#850` from `origin/dev`).

---

## Task 0 — prove the transient window survives a hidden parent (gate)

**The entire architecture rests on this one assumption. Check it before writing
any of the rework.**

GTK4 removed `set_skip_taskbar_hint`; there is no API for it and xdg-shell has no
such protocol. The only lever for "no separate Alt-Tab / dock entry" is
`set_transient_for(library_window)`. But a transient whose parent is hidden may
itself be hidden or unmapped by Mutter — and hiding the library window is exactly
what this design does.

Build a throwaway two-window GTK4 app outside the repo: a parent window and a
transient child. Show both, hide the parent, and observe on the real
Wayland/GNOME session whether the child stays visible, and whether it has its own
Alt-Tab and dock entry.

**Stop condition.** If a transient cannot outlive a hidden parent, **stop and
report** — do not improvise. The fallback is to drop `transient_for` and accept a
second Alt-Tab entry, and that trade was already established as the user's
decision, not the implementer's.

---

## Task 1 — give the compact player its own toplevel

Today `compact_root` (an `adw::ToastOverlay` around `compact_player.handle()`) is
reparented into the shared `WindowContentHost`. Instead, build it once into its
own window, created alongside the library window and kept for the app's lifetime.

Requirements settled in the grill:

- **Closing the mini-player quits the app and saves the session**, exactly as
  today. No new behaviour the user did not ask for.
- **No separate Alt-Tab / dock entry.** It must feel like one app in another
  shape.
- The card keeps what it is today: chromeless, transparent, non-resizable, its
  `CSS_PASSTHROUGH` treatment and its `CARD_MARGIN` shadow room (MINI-1).

The taskbar-entry constraint and its gate are Task 0 — do not start this task
until that gate is green.

---

## Task 2 — switching becomes hide/present

`restore_library()` and `enter_compact()` lose all geometry work:

- to compact: present the mini-player window, hide the library window;
- to library: present the library window, hide the mini-player window.

The library window's size, maximized state and position are never touched by a
mode switch. `apply_compact_metrics()` applies only to the mini-player window and
runs once at construction (plus on decoration-mode change, as today via
`refresh_geometry()`).

Consequences to carry through deliberately:

- `full_width` / `full_height` / `full_maximized` and the whole
  capture-and-restore dance in `enter_compact(capture_full_geometry)` become
  dead. Remove them rather than leaving them inert.
- `geometry_suppressed` exists only because the single window changed size during
  mode switches. Re-derive what, if anything, still needs it — it is shared with
  `session_restore.rs` via `geometry_guard()`, so this is a real coupling, not a
  local cleanup.
- `win.toggle-minimal-view` is a **window** action, and `Ctrl+M` is bound through
  `set_accels_for_action("win.toggle-minimal-view")`. A `win.` action resolves
  against the focused window, so the action must exist on **both** windows or the
  shortcut dies in whichever window lacks it. `compact_mode_suggestion.rs` also
  points a toast at that action name.
- Startup already builds the library tree, so building the library window
  unpresented costs no more than today. When the persisted mode is compact
  (`StartupOpenIntent::CompactPlayback` or the stored `WindowViewMode`), present
  only the mini-player window.
- `GtkApplication` keeps the app alive while windows are *registered*, not while
  they are visible — hiding one is safe. Confirm both windows are added to the
  application.
- The always-on-top menu item is X11-only (`compact_mode_controls::is_x11`);
  keep that behaviour attached to the mini-player window.
- Check MPRIS and the primary menu still work from the mini-player window.
- **Confirm the library window keeps its `WindowContentHost` unchanged** and that
  only the compact card moves out. `set_compact()` and `additional_height()`
  currently live on the shared host and are entangled with the compact path; if
  they cannot cleanly follow the card into its own window, that is an edit
  surface this task must name before starting.

Note for later: `enable-animations` is on, so *presenting* a hidden window may
still get a Mutter map animation. That does not reintroduce grow-from-corner —
the window appears at full size, in place — but if the user still reports "not
instant" after this lands, that is where to look next.

---

## Task 3 — the notify subscriptions that never fire

`wire_full_geometry_tracking` (`minimal_view.rs:326`) and `wire_geometry_tracking`
(`session_restore.rs:201`) both subscribe to `["width", "height", "maximized"]`.
**A `GtkWindow` has no `width` or `height` property** — they are `default-width`
and `default-height`. GObject does not validate a notify detail at connect time,
so two of the three subscriptions fail silently at each site.

Confirmed in the measurement run: the only properties that ever notified were
`maximized`, `default-width` and `default-height`; the tracking closure fired 7
times, every one from `maximized`.

- **`minimal_view.rs`** — becomes dead with task 2; delete it.
- **`session_restore.rs` is a live user-visible bug.** Its tracked value feeds
  `geometry_for_save`, which on close returns `(tracked.0, tracked.1, true)` when
  the window is maximized — the size to restore next time. Since a plain resize
  never notifies, `tracked` only updates on an *unmaximize* transition.
  **Consequence: drag-resize the window, maximize it, close → the resized size is
  lost.**

The simplest correct fix is also a deletion: on a maximized GTK4 window,
`default_width()` / `default_height()` already report the size it would restore
to, so `geometry_for_save` can read them directly and the tracking helper
disappears. **Verify that property behaviour before relying on it**; if it does
not hold, subscribe to the correct property names instead.

**Acceptance:** an xvfb display test covering resize → maximize → close → reopen,
asserting the resized size comes back. This is a real regression today and must
not be closed on a claim.

---

## Task 4 — retire and replace the pinned test

`library_root_is_mounted_before_full_geometry_is_requested` (`minimal_view.rs`,
`#[ignore]`, xvfb) pins an ordering that no longer exists. Do not invert it —
replace it with the invariant that actually protects the user:

**a mode switch never changes the library window's size, maximized state, or
position.**

That test would have caught this bug and keeps its value regardless of how the
switch is implemented later. Keep the existing transition, compact, decoration
and workspace tests green.

---

## Task 5 — keep the measurement harness as a dev hook

The frame probe (`add_tick_callback` logging each presented frame with the
toplevel size) and the `cycle` mode of `REPRISE_SMOKE_MINIMAL_VIEW` currently
live as throwaway instrumentation in
`/home/marvin/Projects/reprise/.worktrees/measure-compact-full-transition`.
The acceptance criteria depend on them and the next person here will need them.

Land them as a permanent, env-gated dev hook in the style the repo already uses
(`REPRISE_SMOKE_*`, `REPRISE_PERF_STARTUP_REPORT`) — off by default, zero cost
when unset. Do **not** ship the ad-hoc `tracing::info!` calls currently scattered
through `restore_library()`.

---

## Verification

- `cargo clippy --all-targets -- -D warnings` and the crate's tests.
- The xvfb tests from tasks 3 and 4.
- **Frame trace with a control arm.** Same harness, same session, ≥ 6 toggles,
  against a control build from the same base commit. Required results:
  - **zero** frames in which the library content is presented at a size other
    than the library window's own steady size;
  - the library window's size, maximized state and position are byte-identical
    before and after a compact round trip;
  - **toggle → first library frame ≤ ~33 ms** (one to two frame intervals at
    60 Hz), as an absolute bound on the new build. The old path's 164–209 ms is
    "before" context, **not** the gate: with no remount and no resize the new
    design would clear that number trivially while proving nothing about whether
    the switch is actually instant.
- Manual visual check on the real Wayland session for the two things no test
  covers: the card's transparency and shadow (MINI-1), and the absence of a
  second Alt-Tab / dock entry.

A number without a control arm proves nothing here — the machine is shared with
other sessions and timings drift.

Record the results in `compact-to-library-transition.findings.md` under an
"After" section, beside the existing "before" table.

---

## Parallelität

**Candidate cut considered and rejected.**

The file-based split — `session_restore.rs` (task 3's persistence half) against
`compact/*` and `window/*` (tasks 1, 2, 4, 5) — has genuinely disjoint file
groups and would merge cleanly. It is still wrong here:

- Task 2 removes `geometry_suppressed`, which is **shared** with
  `session_restore.rs` through `geometry_guard()`. The two strands would be
  reasoning about the same lifetime from opposite ends, and the seam is exactly
  where a silent geometry regression would hide.
- Both halves of task 3 are the same bug at two sites, and the decision
  ("delete, or subscribe to the correct names?") hinges on one property
  behaviour. One implementer answering it once beats two answering it
  separately.
- Tasks 1 and 2 are one change described in two parts.

**Conclusion: one strand.** The cut was attempted and there is no useful one.

No merge order, no cross-strand comparisons, no post-merge cross-checks.
