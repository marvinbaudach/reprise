---
slug: refactor-wave-2026-10-c
worktree: /home/marvin/Projects/reprise-refactor-wave-2026-10-c
branch: feature/refactor-wave-2026-10-c
phase: reviewed
codex_session:
created: 2026-10-04
---
# Refactor wave 2026-10 — strand C

Mother plan: `docs/plans/refactor-wave-2026-10.md`. Its "Standing rules for every strand" bind this strand.

## Strand C — GTK hotspot headroom (`reprise-gnome` only)

**Purpose.** Give the most-changed frontend files room to grow by extracting cohesive siblings.
Moved code is moved verbatim: no reordering, no renaming, and no borrow scope changes.

**Owns:**

- `ui/playback/`:
  - `player_controller.rs`
  - new `player_controller_build.rs`
  - new `player_controller_seams.rs`
  - `queue_transport.rs`
  - new `queue_edit.rs`
  - `mod.rs`
  - the `include_str!` scan lines only in `queue_transport_tests.rs`,
    `playback_history_transport.rs`, `up_next_transport.rs`, `now_playing_wiring.rs` and
    `external_media_completion.rs`
- `ui/preferences/`:
  - `preferences.rs`
  - new `preference_playback_page.rs`
  - `mod.rs`
  - the scan lines only in `preferences_window.rs`
- `ui/window/`:
  - `library_shell.rs`
  - new `library_shell_tests.rs`
  - new `active_content_focus.rs`
  - `window.rs`
  - new `window_first_paint.rs`
  - `mod.rs`
- `ui/style/`: `mod.rs` and new sibling test files.

There is no overlap with strand A. None of these files calls the query functions (verified by
grep).

**C1 — `style/mod.rs` (707 lines).**

- **Tests out.** Move the inline `mod tests` and `composed_css_tests` into sibling files. Follow
  how neighbouring folders declare sibling test modules. `check-architecture.sh` forbids flat
  `#[path]` hacks, so check its rule first.
- **`app_css()` stays explicit and ordered.** List order is the CSS cascade. No self-registration.
- **`CssProvider::new` stays in `mod.rs`.** It is allowlisted to that file.
- **Scan.** `playback/now_playing_wiring.rs:541` reads `../style/mod.rs`. Check whether its
  assertion is positive or negative and keep it meaningful.

**C2 — `window/library_shell.rs` (728 lines).**

- Move the tests to `library_shell_tests.rs`. `library_chrome_tests.rs` sets the pattern.
- Move `ActiveContentFocus` and `focus_widget_or_descendant` to `active_content_focus.rs`.
- **Scan.** The `window_breakpoints_never_own_split_view_collapse` test reads `library_shell.rs`,
  `responsive_side_panels.rs` and `../compact/compact_mode_suggestion.rs` and asserts no
  breakpoint setter. Add `active_content_focus.rs` to that list. Other `include_str!` paths in the
  moved tests stay relative to the same directory.

**C3 — `playback/player_controller.rs` (798 lines, 2 lines of headroom).**

- **What moves.** `new()` goes to `player_controller_build.rs`. The injection setters and
  `connect_*` go to `player_controller_seams.rs`. Both are additional `impl PlayerController`
  blocks.
- **Wiring order.** Do not reorder anything inside `new()`: signal-wiring order is load-bearing.
- **What stays.** The module-header borrow-discipline documentation and `present_track` stay in
  `player_controller.rs`. `up_next_transport.rs:434` splits the file at
  `pub(in crate::ui) fn present_track`.
- **Scan.** `now_playing_wiring.rs:540` asserts that retired cover-accent names are absent from
  `player_controller.rs`. Extend that scan to both new files.
- Target: under 600 lines.

**C4 — `playback/queue_transport.rs` (772 lines).**

- **What moves.** Extract `queue_edit.rs` with `move_queue_rows_to_top`, `clear_play_next`,
  `remove_queue_rows`, `reorder_queue_rows`, `jump_to_queue_row` and the pure helpers they own.
  `queue_transport_projection.rs` and `queue_context_window.rs` set the pattern.
- **What stays, and why.**
  - `fn next` and `pub fn play_from_view` stay adjacent in `queue_transport.rs`, because
    `playback_history_transport.rs:354` splits there.
  - `fn purge_queue_ids` stays in `queue_transport.rs`, because `queue_transport_tests.rs:82`
    scans for it.

**C5 — `preferences/preferences.rs` (782 lines).**

- **What moves.** `playback_page()` and its helpers go to the new `preference_playback_page.rs`:
  `set_gapless_enabled`, `set_crossfade_seconds`, `replay_gain_from_index`,
  `crossfade_value_label` and `GaplessControlState`.
- **Name.** Do not use `preference_playback.rs`. It already holds `EqualizerSurface` and is
  registered in `style::app_css`.
- **Scan.** `preferences_window.rs:395` scans `preferences.rs` for navigation-row focus. Verify
  that its needle stays in `preferences.rs`.

**C6 — `window/window.rs` (581 lines, cap 600).**

- **What moves.** The startup-report and first-paint block at the end of `build()`, about lines
  525–581, goes into `window_first_paint.rs` with the statements in the same order.
- **What stays.**
  - The literals `source_views.wire_episode_played(player)` and
    `source_views.wire_episode_position(player)`, which `external_media_completion.rs:271/307`
    assert.
  - The comments that state ordering invariants: close-request order, and that the nav-history
    listener exists before `session_restore::restore_runtime`.
- **Bail-out.** If the block cannot move without reordering, skip C6 and say why.

**Verification (C):**

```
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test -p reprise-gnome
scripts/check-architecture.sh
```

The ignored display tests guard the moved GTK code. The orchestrator runs
`scripts/check-merge-readiness.sh` after the code phase, so Codex does not run it.
