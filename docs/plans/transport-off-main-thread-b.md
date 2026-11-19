---
slug: transport-off-main-thread-b
worktree: /home/marvin/Projects/reprise-transport-off-main-thread-b
branch: feature/transport-off-main-thread-b
phase: shipped
codex_session:
created: 2026-11-11
---
# Strand B: a failed start never touches the track's file on the main thread

Strand B of `docs/plans/transport-off-main-thread.md` (closes #1327 together with
strand A). Read the mother plan first. Line numbers are from `origin/dev` at
`003ae49fd3`.

## Problem

Strand A takes the player's GStreamer calls off the main context. A track on a
dead mount still ends in the frontend's fault handler: the start watchdog gives up
after about 24 s and emits `Error`, and a synchronous `play` failure does the same
at once. Both channels reach `handle_unplayable_track`
(`crates/reprise-gnome/src/ui/playback/playback_faults.rs` 88ff.), which runs on
the main thread:

- `Path::new(&summary.path).is_file()` (96), to choose between "missing" and
  "could not play";
- `queries::mark_track_missing_if_current` (`reprise-core`
  `queries/maintenance_missing.rs` 25–58), which runs `std::fs::metadata` on the
  track, the parent probe in `reachability`, and the `symlink_metadata` loop in
  `mounts.rs` 65–80.

On a dead hard mount each of these blocks in the kernel, so the window freezes.
The success path is already clean (scout, 2026-11-11): cover, waveform, lyrics
and the notification thumbnail all load off the main thread.

## Design (grill decisions)

1. **Skip at once.** `handle_unplayable_track` calls `skip_after_failure` without
   waiting for any probe. The skip counter and its limit stay as they are.
2. **Probe off the main thread.** `is_file()` and `mark_track_missing_if_current`
   run together in one `gio::spawn_blocking` job, which takes everything it needs
   by value (track id, path, a DB handle that may be used off the main thread).
   No `Rc`, widget or `RefCell` borrow crosses into it.
3. **Apply the answer on the main thread.** When the job answers:
   - file missing and marked (`Ok(true)`): `reload_track_list()` and the toast
     `TRACK_UNAVAILABLE_SKIPPED`, exactly as today (FB-6 `[active]` stays
     unchanged);
   - file present: the toast `could_not_play_toast(title)`, as today;
   - DB row gone or query failed: no toast, as today.
   `mark_track_missing_if_current` already checks that the row still has the same
   path, so a late answer cannot mark a different track.
4. **A probe that does not answer.** If the job has not answered within
   `FAULT_PROBE_WAIT` (about 1 s, a named constant), show the neutral
   `could_not_play_toast(title)` at that moment and ignore the job's toast when it
   answers later; it may still mark the track missing and reload the list. A
   probe that never returns leaves the track unmarked, and nothing freezes. One
   fault shows exactly one toast.
5. Both callers keep calling `handle_unplayable_track` the same way: the
   synchronous `play` error in `present_track` and the bus `Error` in
   `player_event_handling.rs`. No string changes, so no gettext catalog changes.

To make the probe testable, give the handler a seam for the probe (a function
value or a small trait the controller holds, defaulting to the real
`is_file` + `mark_track_missing_if_current`). Keep it in the same directory.

## Tasks (test-first)

1. Write the tests below and run them on the unchanged code. The freeze test must
   fail (the injected probe blocks the main thread). Record how.
2. Implement the design.
3. Update the module doc of `playback_faults.rs`: which part runs where, and the
   one-toast rule.

## Tests

- **Freeze test** (`[gtk]` display test, `#[ignore]` like the other display
  tests): an injected probe that blocks until released. A fault of the current
  track skips to the next one at once, and a main-context heartbeat never goes
  longer than 500 ms without firing. After `FAULT_PROBE_WAIT`, exactly one
  neutral toast is shown; releasing the probe afterwards adds no second toast.
- Missing file (probe answers "missing", mark `Ok(true)`): skip, the track is
  marked, the list reloads, one `TRACK_UNAVAILABLE_SKIPPED` toast.
- Present file (probe answers "present"): skip, no mark, one "could not play"
  toast.
- Row gone: skip, no toast.
- The skip limit still stops playback with the single "too many unplayable"
  toast when every queued track faults.
- Existing `playback_fault*` tests keep passing; adjust one only to wait for the
  probe's answer, never to weaken an assertion, and list each adjusted test.
- **Mutation proof** for the freeze test: call the probe inline again, see the
  test fail, restore. Report it.

## Verification scope (overrides AGENTS.md's whole-workspace gate for this run)

Prefix each cargo command with `heavy-run heavy --`:

- `cargo fmt --check`
- `cargo clippy -p reprise-gnome --all-targets -- -D warnings`
- `cargo test -p reprise-gnome --bins playback_fault` (filtered). Do **not** run
  the unfiltered `reprise-gnome` suite and do not block on it: its `settle()`
  family flakes under load and is not this change's. The orchestrator runs it.
- The new display test alone, through `scripts/check-display-tests.sh` with that
  test's name if the script accepts a filter; otherwise say so and leave it to
  the orchestrator. Never run the app or a display test outside the isolated
  recipe in AGENTS.md.
- `scripts/check-crate-layers.sh`

## Known residuals

- The real dead mount stays a manual check: only the injected probe proves this
  strand.
- A probe that never returns keeps one `spawn_blocking` worker for as long as
  the kernel call lasts.

## File ownership

- `crates/reprise-gnome/src/ui/playback/playback_faults.rs`
- its tests (`crates/reprise-gnome/src/ui/playback/playback_fault*_tests.rs`, or
  a new sibling test file)
- a probe seam, if needed, in `crates/reprise-gnome/src/ui/playback/`
- the controller field that holds the seam, only where it is constructed
- this strand file

Change nothing in `reprise-core` (`mark_track_missing_if_current` is only called)
or `reprise-platform-linux`, and no `docs/ux-rules.md` entry.
