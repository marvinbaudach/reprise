---
slug: the-real-window-passes-the-ux-audit
worktree:
branch:
phase: planned
codex_session:
created: 2026-09-15
strands: a,b
merge_order: a,b
---
# The real window passes the UX audit

Follow-up round of `every-view-passes-the-ux-audit` (A #949, B #950, C #967,
fix-forward #968; all on `origin/dev` = `592260c003`). Mother plan, frozen at the end of the plan phase (2026-09-15). Strand files:
`the-real-window-passes-the-ux-audit-a.md` (the real window) and `-b.md` (the
leftovers). Each strand carries this file unchanged.

## Source and goal

The post-merge tour of 2026-09-15 (Xvfb + openbox, real-library snapshot,
dev `1f47d431a9` against control `4a0b3a49eb`) found three defects that every
rule-named test misses because the tests build a *fixture* while the defect
lives in the *production assembly*:

1. **The window's true minimum height is 635 px, `MIN_HEIGHT` says 400.**
   dev at 1024×600 logs `AdwToolbarView exceeds AdwApplicationWindow height:
   requested 635 px, 590 px available` 275 times; the control arm logs it
   zero times (its minimum was 512 at 600×400). Strand A raised the minimum
   — LIBRARY floor `LIBRARY_BLOCK_MIN_HEIGHT = 226` (`sidebar_navigation_scroller.rs`)
   plus a player bar that no longer shrinks — and left `MIN_HEIGHT = 400`
   (`window_bootstrap.rs:12`) and the core clamp `window_height.clamp(400, 8192)`
   (`session_normalization.rs:82`) untouched, because
   `style_5_player_bar_survives_the_minimum_window` passes on a shell with an
   empty sidebar and a `Label` for content (`window_layout_tests.rs`). At
   600×400 the player bar is entirely off-window; at 1024×600 45 px of the
   sidebar's pinned block are cut.
2. **A ~93 px empty band sits between the pinned ISSUES/card block and the
   player bar** at 1280×720 and 1024×600, on both arms. `sidebar_root.rs`
   stacks `scrolled` (vexpand) and `issues_section` (vexpand false, valign
   End) in a vexpand box; that box is the child of an `adw::NavigationPage`
   in the `OverlaySplitView` (`library_shell.rs:442`, `:294`). Fixture test
   `fb_8_progress_region_reaches_split_view_bottom` is green. So the slack is
   held by something only the production assembly adds: `ToastOverlay` →
   `LibraryPlayerBarShell` → split view (`window.rs:361-366`), the
   `WindowContentHost` toolbar view (`window_decorations.rs:29`), the
   `show_sidebar(false)`-then-shown sequence (`library_shell.rs:298`,
   `window_navigation.rs:15`), the production stylesheet, or the
   session-sized window. Nobody has measured which; AT-SPI from a second
   process could not reach the tree.
3. **At 1280×720 the Length column's values are clipped at the window edge**
   (`3:4|`, `3:1|`) while the header "Length" is whole — the table overflows
   its viewport by ~15 px. The control arm shows `3:47` with a margin. A
   regression from #949's fixed widths / right alignment (STYLE-14) that
   `style_6_the_table_never_overflows_its_viewport` misses at 700/1000/1600
   px on a bare `ColumnView`. Evidence: `postmerge/tour-r1280/01-library.png`
   right edge, versus `tour-control-r1280/02-sidebar-scrolled.png`.

Plus the follow-ups the mother plan parked because their files were in
flight: equalizer profile row, podcast entities, Recently Played order,
sidebar labels in header case (GP-21's "later change"), the Recently Added
icon, QUE-2a, NAV-16 clean-up, and A3.4's whole-row playing highlight, which
landed as a per-cell tint (`.reprise-track-cell.now-playing`).

Goal: the *real* window — built by `ui::window::build`, the function
`main.rs` calls — holds its player bar at an honest minimum, leaves no
phantom height in the sidebar and no clipped column at 1280×720; and the
parked items ship. Presentation and data hygiene only; no MCP surface
changes (the smart-list rename is data every client already reads by id).

## Decisions taken in the grill (binding, 2026-09-15)

- **D1 — Measure the real assembly, not a fixture.** A `cfg(test)` hook
  beside `window_online_module_test_hook.rs` publishes the composed window's
  handles; display tests build the window through `surface::build` exactly
  as `window_online_module_effects_tests.rs` does (registered
  `adw::Application`, `test_db::open()`, `StartupOpenIntent::Library`).
  Alternative considered: an out-of-process probe on the binary
  (`REPRISE_SMOKE_LAYOUT_PROBE=<json>`); rejected because the in-process
  route can seed the sidebar's worst case and assert on widget handles, and
  the runner already provides D-Bus, Xvfb and `fakesink`.
- **D2 — `MIN_HEIGHT` becomes the measured minimum** of the real assembly
  with the LIBRARY floor, the ISSUES heading with one row, one running card,
  the player bar and the header bar, rounded up to the next 10 — the
  number comes from the test run under `scripts/check-display-tests.sh`,
  never from estimation (expected ≈ 540). Ceiling **560**: a maximised
  window on a 1024×600 display under GNOME's top bar. If the measurement
  exceeds 560 the strand stops at 560, lets the pinned block scroll, and
  says so. The core clamp follows through one shared constant. STYLE-5's
  "600 × 400" is rewritten. Alternative: keep 400 and let the pinned block
  squeeze to ~30 px — rejected, the block is unreadable there.
- **D3 — The pinned block is what yields; the LIBRARY floor never does.**
  The block becomes a natural-height scroller (no floor, scrolls when the
  window is short). Its natural height stays exactly what it paints
  (FB-8), so the band cannot come from it and three running cards never
  raise the window's minimum. New FB-15.
- **D4 — The clipped Length column is a regression and in scope**, fixed
  where the fit lives (`track_list_column_widths*.rs`, `table_columns/**`),
  proven on the real assembly.
- **D5 — The whole-row highlight moves into strand A** (it owns the column
  files) and gets a rule, PLAY-17, instead of the "plain widget test" the
  previous plan allowed.
- **D6 — Sidebar labels in header case via one migration (v85)** renaming
  the seeded smart lists by their rules/role, never by name alone, plus the
  three sentence-case constants in `strings_sidebar.rs` and the gettext
  catalogues in the same commit.
- **D7 — A smart place opens in its own order (BROWSE-15).** Today
  `queries/smart.rs` wraps the member order in the view's persisted sort
  (`ORDER BY {view_order}`, line 73) and `default_sort_for_source(Smart(_))`
  returns `None` (`track_list_sort.rs:165-181`), so "Recently Played" shows
  the artist sort the user last clicked in Music. Entering a smart place
  resets the view sort to the list's `sort_field`/`sort_dir`; a header click
  still sorts within the view until the place changes.
- **D8 — Two strands, merge order A then B.** All eight taken as recommended.

## Rulebook discipline (binds every strand)

- A behaviour change flips or adds a rule in the same commit as its
  rule-named test. IDs are append-only; next free IDs are named per task and
  no strand uses an ID the other strand's file names.
- Geometry is proven by result (STYLE-1): `compute_bounds`/`measure` on the
  real assembly, never "property X is set".
- `scripts/check-display-tests.sh --rule-named --shard N/M` is the runner
  that counts: each test in its own process, fresh XDG dirs,
  `dbus-run-session`, cairo, **no window manager** (memory
  `a-display-geometry-repro-needs-the-jobs-own-environment`). A bare
  `xvfb-run cargo test` pass is not evidence. Under no WM the CSD shadow
  eats 5 px per edge: request the size, then poll until the window's own
  allocation reports it (the SET-19 lesson).
- `scripts/check-ux-traceability.sh` green per strand; `docs/ux-rules.md`
  sections are owned per strand (below); a strand appends at the end of a
  section and never touches the other's sections.

## Follow-ups (not in this plan)

- The app instance that vanished without a log line after a wheel scroll
  over Preferences' Background Activity bar (not reproduced; the bar has no
  scroll controller — `preference_background_bar.rs`).
- `table_columns/registry.rs`'s pointer-keyed thread-local → a threaded
  handle (cleanup, no user-visible change).
- Keyboard and AT-SPI audit, compact mode, cover-cloud at low resolution.

## Parallelität

Two strands, disjoint by file, each one Codex run in its own worktree from
`origin/dev`:

- **A — the real window:** `ui/window/**`, the sidebar layout files
  (`sidebar_root.rs`, `sidebar_issues_section.rs`, `sidebar_activity_slot.rs`,
  `sidebar_navigation_scroller.rs`, `sidebar_layout_tests.rs`, `sidebar.rs`),
  `ui/player_bar/**`, `ui/track_list/track_list_columns*.rs`,
  `track_list_column_widths*.rs`, `table_columns/**`,
  `track_list_row_interaction.rs`, `ui/style/**`,
  `reprise-core/src/library/session.rs`, `session_normalization.rs`.
  Rulebook G, U, C. Tasks A0–A5.
- **B — the leftovers:** `ui/preferences/preference_equalizer*.rs`,
  `preferences_tests.rs`, `reprise-core/src/podcasts/**`,
  `reprise-core/src/db.rs` + the v85 file, `reprise-core/src/queries/smart.rs`,
  `ui/track_list/track_list_sort*.rs`, `track_list_reload.rs`,
  `ui/strings_sidebar.rs`, `ui/sidebar/sidebar_presentation.rs`,
  `sidebar_rebuild.rs`, `ui/releases/releases_presentation.rs` (gp_21 test),
  `ui/now_playing/up_next_panel*.rs`, `po/**`. Rulebook F, AF, Z, AI, J, B.
  Tasks B1–B7.

Seams checked: `ui/track_list/` is split by file, not by directory — A's
re-fit hooks on the viewport, never on reload; B's sort change never
touches the column files. Both touch `docs/ux-rules.md` in different
sections. A's real-window tests assert geometry, not names, so B's rename
does not move them; B's `gp_21` test asserts strings, not geometry.

**Merge order: A, B.** No strand's verification reads the other's files;
B rebases onto A's dev in seconds.

**Post-merge cross-checks** (read files no single strand owns):
1. The headless tour on dev after B (recipe `postmerge/start-tour.sh`,
   real-library snapshot) at 1280×720, 1024×600 and `600 × MIN_HEIGHT`:
   zero `exceeds AdwApplicationWindow` lines in `app.log` at every size;
   no band under the pinned block; Length values whole at 1280; the player
   bar whole at the minimum; the three seeded smart lists in header case;
   Recently Played newest-first on the snapshot.
2. `scripts/check-ux-traceability.sh` green on dev after B.
3. A's real-window tests green on the rebased B (B's migration changes the
   seeded names the test DB carries).
4. The CI Arch container's rust (1.98.x, memory
   `ci-installs-arch-rust-at-job-time`) accepts both branches' new test
   files — watch the dev run that completes and still contains B.
