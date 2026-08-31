---
slug: the-sidebar-keeps-its-column
worktree: /home/marvin/Projects/reprise-sidebar-column
branch: feature/the-sidebar-keeps-its-column
phase: shipped
codex_session:
created: 2026-08-28
---
# The sidebar keeps its column

## Goal

The left library sidebar must never disappear because of the window width. It
is a structural column, not a responsive one: only the user (through the header
toggle or the stored preference) decides whether it is there.

## What made it collapse

Two mechanisms remained on `dev` after #727 removed the width-driven close of
the left panel:

1. `ui/window/library_shell.rs::build_split_view` wrapped the library
   `AdwOverlaySplitView` in an `AdwBreakpointBin` whose `max-width: 799px`
   breakpoint set `collapsed = true`. Below 800 px the sidebar stopped being a
   column and became an overlay that `apply_sidebar_visibility` then kept
   hidden.
2. `ui/window/responsive_side_panels.rs` made the two flanks mutually exclusive
   below `CONSTRAINED_WIDTH` (1300 px). `visibility_after_opening(NowPlaying)`
   returned `library: false`, so opening the now-playing panel in a constrained
   window took the sidebar away.

## The change

- Drop the breakpoint bin. `build_split_view` returns the split view itself,
  built `collapsed(false)` and `pin_sidebar(true)`, with no breakpoint that
  writes `collapsed`. `LibraryShell` loses its `root` field; `window.rs` passes
  the split view to `LibraryPlayerBarShell::new` instead.
- The constants `SIDEBAR_BREAKPOINT_WIDTH` / `SIDEBAR_COLLAPSE_WIDTH` go with
  it. `now_playing_column::INFO_PANEL_COLLAPSE_WIDTH` owns its own 799 px
  threshold now — it measures the content pane, which the pinned sidebar no
  longer hands back.
- The exclusion in `responsive_side_panels.rs` runs one way only: opening the
  sidebar still closes the now-playing panel, opening the now-playing panel
  leaves the sidebar where the user left it. `visibility_after_opening` and
  `visibility_change_target` take the current visibility so the target can
  preserve it, and return `None` when nothing changes.
- `docs/ux-rules.md` STYLE-7 and the `window.rs` module doc say what the code
  does.

## Verification

- `cargo clippy -p reprise-gnome --all-targets -- -D warnings`: clean.
- `cargo test -p reprise-gnome --bins`: 2081 passed, 0 failed.
- `scripts/check-display-tests.sh`: 819 of 820 passed. The single red,
  `ui::window::window_navigation::tests::wide_window_toggle_collapses_and_restores_the_sidebar_column`
  (`window_navigation.rs:551`), fails identically on the unchanged base commit —
  a pre-existing dev red, verified with a control arm.
- New display test `the_sidebar_keeps_its_column_at_a_narrow_viewport`: at a
  600 px window the split is not collapsed and the sidebar still reserves a
  content slot. Control arm: the same widgets wrapped in the removed
  `AdwBreakpointBin` do collapse at 600 px, so the assertion discriminates.
- Minimum window width measured headless (xdotool, requests of 500 and 320 both
  settle at 600x800): unchanged at the 600 px the window already declares in
  `window_bootstrap.rs:11`. The pinned column does not raise the floor, and a
  screenshot at 600x800 shows the full sidebar beside unclipped content.

## Parallelität

No cut. The change is four files that all describe one seam — the library
split view and the panel constraint that reads it — and `library_shell.rs`,
`responsive_side_panels.rs` and `now_playing_column.rs` have to move together
(the removed constants are shared). A second strand would own no disjoint file
group.
