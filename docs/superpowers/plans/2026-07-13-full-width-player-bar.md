# Full-Width Library Player Bar — Implementation Plan

**Spec:** `docs/superpowers/specs/2026-07-13-full-width-player-bar-design.md`

**Baseline:** 664 passing workspace tests; 16 display-only tests ignored by
the normal workspace run.

## Global constraints

Use RED→GREEN TDD for every behavior change. Code, comments, logs, errors, UI
strings and commits stay English. Never touch real music or the real database.
Every app/display run uses private D-Bus, Xvfb, scratch `XDG_DATA_HOME` and
`XDG_CACHE_HOME`, forced X11, unset Wayland and `REPRISE_AUDIO_SINK=fakesink`.
`reprise-core` remains free of gtk4/libadwaita/gstreamer/zbus. Every created or
substantially edited file ends below 800 lines. `window.rs` and `player_bar.rs`
are edge-tight, so layout logic must move into focused siblings. Before every
implementation commit run fmt, strict clippy, workspace tests and audit. Never
push.

## Task 1 — Move the library player bar outside the pane split

**Files:**

- new `crates/reprise-gnome/src/ui/library_player_bar.rs`
- `crates/reprise-gnome/src/ui/mod.rs`
- `crates/reprise-gnome/src/ui/window.rs`
- `crates/reprise-gnome/src/ui/minimal_view.rs`
- `crates/reprise-gnome/src/ui/compact_mode_controls.rs`
- `crates/reprise-gnome/src/ui/preferences.rs`
- `crates/reprise-gnome/src/ui/window_smoke.rs`

**Interfaces:**

```rust
#[derive(Clone)]
pub(super) struct LibraryPlayerBarShell { /* root + bar block */ }

impl LibraryPlayerBarShell {
    pub(super) fn new(
        content: &impl IsA<gtk4::Widget>,
        status: &impl IsA<gtk4::Widget>,
        player_bar: Option<&impl IsA<gtk4::Widget>>,
        position: PlayerBarPosition,
    ) -> Self;
    pub(super) fn widget(&self) -> &gtk4::Box;
    pub(super) fn set_position(&self, position: PlayerBarPosition);
}
```

`MinimalView` consumes a `&gtk4::Widget` full root instead of a concrete
`AdwNavigationSplitView`. Preferences and the smoke hook receive a cloned
`LibraryPlayerBarShell` and call `set_position`; `window::apply_bar_position`
is removed.

**TDD steps:**

1. Add one ignored RED display test in the new module. Construct a 1000-px
   window with a representative content widget, status and player bar. Assert
   content and bar block are direct root children; after event-loop settling
   the bar block/root allocated widths match; Bottom makes it the last child,
   Top the first; repeated changes retain exactly one parent.
2. Run the exact test in its own fully isolated Xvfb process and observe the
   missing module/interface failure.
3. Implement the vertical root and safe remove/prepend/append transition.
   Give content `vexpand=true`; never put the global block back into the inner
   ToolbarView.
4. Generalize `MinimalView`'s full root, then replace window, Preferences and
   smoke call sites. Keep the actual `split_view` for navigation callbacks and
   hand only the shell root to Library/Compact switching.
5. Re-run the display test (17 ignored total), targeted Minimal/Preferences
   tests, all gates and an adversarial parent/lifetime/Top-Bottom review.

**Commit:** `refactor: span library player bar across full window`

## Task 2 — Restore the three-zone player-bar arrangement

**Files:**

- new `crates/reprise-gnome/src/ui/player_bar_layout.rs`
- `crates/reprise-gnome/src/ui/mod.rs`
- `crates/reprise-gnome/src/ui/player_bar.rs`

**Interfaces:**

```rust
pub(super) struct PlayerBarWidgets {
    pub(super) root: gtk4::ActionBar,
    // existing cover, labels, transport, seek and secondary controls
}

pub(super) fn build() -> PlayerBarWidgets;
```

The builder creates start Track, center Transport+Seek, and end Secondary
zones. `PlayerBar` retains all state, guards and callbacks and merely owns the
returned widget handles.

**TDD steps:**

1. Add one ignored RED display test in the new module. It asserts Cover/title/
   artist ancestry in the start zone; Previous/Play/Next in the transport row;
   position/scale/duration in the seek row; Shuffle/Repeat/Volume in the end
   zone; no secondary control is a descendant of the center zone. Measure the
   root at 1200 px and assert the seek scale receives positive width.
2. Run the exact test in its own fully isolated Xvfb process and observe the
   missing module/interface failure.
3. Extract widget construction from edge-tight `player_bar.rs`. Compose the
   middle as a vertical box with a centered Previous/Play/Next row and a
   flexible time/seek/time row. Pack Shuffle/Repeat/Volume into the end zone.
   Preserve every tooltip, accessible label, sensitivity default and CSS hook.
4. Re-run the display test (18 ignored total), all PlayerBar/seek/controller
   tests, all gates, file-size checks and an adversarial input/state review.

**Commit:** `feat: restore three-zone library player bar layout`

## Task 3 — End-to-end QA and close-out

**Files:**

- `docs/agent-workflow/MANUAL-QA.md`
- `.superpowers/sdd/progress.md`
- `docs/agent-workflow/STATUS.md`

**Steps:**

1. Run both new display tests individually plus all 18 display-only tests in
   separate isolated Xvfb processes.
2. Build the current debug binary and run the existing mapped-pointer harness
   unchanged. Confirm the wide Library screenshot shows the player bar across
   sidebar/content/information boundaries and the log rejection remains clean.
3. Run isolated Top and Bottom smoke starts, then Compact→Library round-trip;
   prove the global bar returns with the saved position and playback state is
   unchanged.
4. Run `scripts/check-release.sh`, standalone core build/purity, audit and
   explicit touched-file line counts.
5. Whole-stage adversarial review against the spec. Fix Important/Critical
   findings with RED regressions and separate fix commits, then repeat affected
   checks.
6. Record native GNOME visual checks in `MANUAL-QA.md`, append exact evidence
   to the ledger, set STATUS to plan complete/current-plan-none and release the
   lock.

**Commit:** `docs: record full-width player bar QA`
