---
slug: community-launch-a
worktree: /home/marvin/Projects/reprise-community-launch-a
branch: feature/community-launch-a
phase: shipped
codex_session:
created: 2026-10-31
---
# Community launch — strand A: crash reports and Rhythmbox locations

Mother plan: `docs/plans/community-launch.md` (decisions, facts, target values, post-merge checks).
Implements `docs/plans/consolidation-plan.md` §0.1–0.4 with the corrections below, plus the
importer change. Test-first throughout: failing test, see it fail, minimal code, green, gates,
commit.

**Owns:** `crates/**`, `po/**`, `docs/ux-rules.md`, this file. Touch nothing else.

## Corrections to the consolidation plan

- Rule IDs START-3, START-4 and FB-9 are taken. Use **START-6** (log file, crash detection,
  next-start offer), **START-7** (Rhythmbox locations) and **FB-19** (Copy Diagnostics menu entry);
  tests `start_6_…`, `start_7_…`, `fb_19_…`. Do not create `start_4_…` tests:
  `queue_transport_tests.rs:156` already uses that prefix for the session-restore rule.
- `ui/diagnostics.rs` stays the module root (it has the child `diagnostics/tests.rs`). New siblings
  go under `ui/diagnostics/`; `ui/mod.rs:44` already declares the module.
- `init_logging` (`main.rs:122-132`) already composes `registry()`, a stderr layer and
  `ui::diagnostics::session_layer()`, and is called from `run()` (`main.rs:292`), after
  `register_app_resources` and `mark_process_start`.
- `install_panic_hook` exists (`main.rs:134-153`, called at `:293`) and chains the previous hook.
  Extend it; keep the chaining.
- `window.rs` is 541 lines with a 600-line cap and has no close-request handler. Clean shutdown runs
  through `ui/session_restore.rs:137` (`save_once`) and `ui/termination.rs` (SIGTERM).
- A redacted report exists: `diagnostics::build_report(db, db_path)` wraps
  `reprise_platform_linux::diagnostics::build_report` and is shown in About (`about.rs:103`).
- Frontend thinness budgets are rusqlite 115, filesystem 13, threads 14, workers 7
  (`scripts/check-frontend-thinness.sh:88-93`). Keep `std::fs` out of `reprise-gnome`.

## Tasks

**A1 — state root, log paths, rotation (0.1).** A new std-only `reprise-core` module beside
`data_root.rs` and `cache_root.rs` (for example `state_root.rs`), with the `test-private-dirs`
isolation: `log_path()`, `previous_log_path()`, `crash_marker_path()`, `rotate_on_start()`,
`MAX_LOG_BYTES` (8 MB; writes past the cap are dropped; no second rotation), and a pure
`log_path_in(root)`. Location `$XDG_STATE_HOME/reprise/`, falling back to the data directory when
unset. Tests: `log_path_follows_xdg_state_home`,
`log_path_falls_back_to_the_data_dir_when_state_home_is_unset`,
`rotation_keeps_exactly_one_previous_run`,
`rotation_reports_but_survives_a_read_only_directory`.

**A2 — log mirrored to the file (0.2).** A third layer in `init_logging`: `with_ansi(false)`, its
own `EnvFilter` (it is not `Clone`), a size-capped custom `MakeWriter`, rotation before the first
write; startup never blocks on it. Test:
`file_writer_is_absent_when_the_state_directory_cannot_be_created`.

**A3 — running sentinel and one next-start offer (0.3, decision 1).**
- Write the marker at startup; remove it on clean shutdown in `session_restore::save_once` and in
  the `termination.rs` path.
- The panic hook writes message, location and `Backtrace::force_capture()` to the log through a
  `write_report(message, location, backtrace)` that takes strings (`PanicHookInfo` cannot be built
  in a test).
- On the next start, if the marker is present, show one toast offering "Copy Diagnostics", then
  clear the marker. Put the offer in a sibling module, not in `window.rs`.
- Rule **START-6**. Tests: marker present after an unclean exit, absent after `save_once` and after
  the termination path, the offer shown once and not again,
  `crash_report_contains_version_schema_and_the_panic_location`,
  `crash_report_never_contains_a_library_path`, plus the rule-named `start_6_…` tests.

**A4 — Copy Diagnostics in the primary menu (0.4).**
- Extend the existing `diagnostics::build_report` with the enabled modules and the log tail,
  redacted the same way as the warnings. Do not add a second builder.
- `ACTION_COPY_DIAGNOSTICS` in `primary_menu.rs` (constants `:31-45`, entries `:87-97`), following
  the `ACTION_ABOUT` callback pattern (`:210-215`). Copy via the `gdk::Display` clipboard, then a
  toast.
- Update `settings_section_has_help_before_about` (`:458`), which asserts help + 1 == about.
- The label is exactly **"Copy Diagnostics"** (target value; strand C quotes it).
- Rule **FB-19**, tests `fb_19_…`.

**A5 — Rhythmbox locations (START-7).**
- A pure resolver in `reprise-core` (for example `library/rhythmbox_locations.rs`) that takes every
  input explicitly — the env override, `in_flatpak`, `HOME`, `HOST_XDG_DATA_HOME`, the native data
  directory — and returns candidate directories in order:
  1. the env override `REPRISE_RHYTHMDB_PATH` (behaviour unchanged);
  2. inside a Flatpak, `$HOST_XDG_DATA_HOME/rhythmbox` if set, else `$HOME/.local/share/rhythmbox`;
     outside, the native data directory's `rhythmbox`;
  3. `$HOME/.var/app/org.gnome.Rhythmbox3/data/rhythmbox` (Rhythmbox from Flathub), in both cases.
- The first directory containing `rhythmdb.xml` wins; `playlists.xml` is its sibling unless
  `REPRISE_RHYTHMBOX_PLAYLISTS_PATH` is set.
- Add `pub fn running_in_flatpak()` to `reprise-platform-linux` and use it for the new code only.
- `default_rhythmdb_path()` and `default_playlists_path()` delegate to the resolver.
- Table-driven tests for every branch, plus rule-named `start_7_…` tests.
- Target value for strand B: the probed host directories are `~/.local/share/rhythmbox` and
  `~/.var/app/org.gnome.Rhythmbox3/data/rhythmbox`. Do not read or check the manifest here; that
  comparison is post-merge check X1.

**A6 — rules and strings.**
- START-6 and START-7 in `## I. Start state`, FB-19 in `## G. Feedback vocabulary`, each
  `[planned]` with `<!-- REVIEW: rule proposal -->`, at level `[core]` or `[gtk]` (not `[manual]`).
- New strings as `N_!` constants in a `strings_*.rs` file, with German and Spanish translations;
  add any new strings file to `po/POTFILES.in`; run `scripts/update-catalogs.sh`.

## Gate

The full battery from `AGENTS.md`, `scripts/tests/gettext-catalogs.sh`,
`scripts/check-architecture.sh` (thinness budgets, crate layers, panic-lint budgets), and the core
purity proof.
