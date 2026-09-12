---
slug: the-sidebar-toggle-is-always-there
worktree: /home/marvin/Projects/reprise-the-sidebar-toggle-is-always-there
branch: feature/the-sidebar-toggle-is-always-there
phase: planned
codex_session:
created: 2026-09-12
---
# The sidebar toggle is always there

Written from a completed diagnosis (2026-09-12), not from a grilled draft —
the maintainer called `/code` directly. The two red tests already sit on the
branch this worktree starts from; the plan is to turn them green.

## The bug, proven

The left sidebar toggle is missing after a normal start and only appears once
the Layout preference is touched. The maintainer's rule: **the left toggle is
always visible, exactly like the right (info-panel) toggle**, and a click while
the Sidebar layout option is off turns the sidebar back on.

Root cause: `sync_sidebar_toggle` in
`crates/reprise-gnome/src/ui/window/window_navigation.rs` reads
`sidebar_page.is_visible()`. In gtk4-rs that is `gtk_widget_is_visible()` —
true only when the widget **and every ancestor** are visible — not
`get_visible()`, the widget's own flag. The one startup sync runs from
`window_runtime_wiring::wire` (`window.rs:470`) before
`startup_window.present()` (`window.rs:566`), so it always reads `false`,
hides the toggle (`window_header.rs:41` builds it `.visible(false)`), and
nothing resyncs it later: the `visible-notify` handler on `sidebar_page`
watches a property that never changes again.

The same wrong getter at `window_navigation.rs:181` guards the restore of last
session's manual collapse, so a persisted `ui.sidebar_collapsed = 1` is
silently dropped and the sidebar comes back expanded.

Reference implementation for "always visible": the right toggle in
`now_playing/now_playing.rs:376-381` — built without `.visible(false)`, its
`visible` never set, only `active` mirrors state.

## Red tests already on the branch

Both in `window_navigation.rs`, both `#[ignore = "requires a display …"]`:

- `sidebar_toggle_survives_being_wired_before_the_window_is_presented` —
  fails at the `toggle.get_visible()` assertion; after the fix its following
  `toggle.is_active()` assertion must hold too.
- `a_persisted_sidebar_collapse_survives_a_restart` — fails at
  `!split.shows_sidebar()`.

Display tests must run **one process per test**: two in one process abort on
"Attempted to initialize GTK from two different threads". Run each as

```
xvfb-run -a cargo test -p reprise-gnome -- --ignored --exact --test-threads=1 \
  ui::window::window_navigation::tests::<name>
```

`reprise-gnome` has only a bin target — no `--lib`.

## Tasks

1. `window_header.rs`: drop `.visible(false)` from the sidebar toggle builder
   and rewrite the comment above it — the button is always present, like the
   right toggle; only its `active` state is synced. Check the display test in
   the same file (around line 71) for an assumption that it starts hidden.
2. `window_navigation.rs::sync_sidebar_toggle`: never touch `visible`. Compute
   `has_sidebar` from `sidebar_page.get_visible()` (own flag) and set only
   `active = has_sidebar && split_view.shows_sidebar()`.
3. `window_navigation.rs::wire_sidebar_toggle`: the collapse-restore guard at
   ~181 and the `connect_toggled` / `connect_visible_notify` reads switch from
   `is_visible()` to `get_visible()` — the question everywhere is "is the
   sidebar slot enabled", never "is the window on screen".
4. `connect_toggled`: when the button is switched on while the sidebar slot is
   disabled (`!sidebar_page.get_visible()`), turn the slot back on — persist
   `settings::set_sidebar_visible(conn, true)` (the setter
   `preference_layout.rs::apply_window_control` already uses) and call
   `apply_sidebar_visibility(split_view, sidebar_page, true)`; then continue
   with the existing show/persist path. Switching it off while the slot is
   disabled stays a no-op. Keep the `updating` re-entrancy guard intact — the
   `visible-notify` handler this triggers calls `sync_sidebar_toggle`.
5. Delete `sidebar_toggle_is_visible()` (it returns its argument) and its test
   `sidebar_toggle_remains_available_whenever_the_sidebar_slot_exists`.
6. Add one more display test next to the two red ones: build the same tree,
   `apply_sidebar_visibility(…, false)`, wire, assert the toggle is visible and
   inactive; then `toggle.set_active(true)` and assert `sidebar_page.get_visible()`,
   `split.shows_sidebar()`, and `settings::get_sidebar_visible(&conn)`.

## Verification (all three must be green; exit status read directly)

- The three display tests above, each in its own `xvfb-run` invocation.
- `cargo test -p reprise-gnome window_navigation` (the non-display tests in
  that module) and `cargo test -p reprise-gnome window_header`.
- `cargo clippy -p reprise-gnome --all-targets` clean for the touched files.

Do not touch `now_playing_light.rs:105` (same pattern, deliberately out of
scope) nor any other `is_visible()` call outside the functions named above.

## Parallelität

Not cut: every task changes `window_navigation.rs` or the one builder in
`window_header.rs` that it syncs — no disjoint file group.
