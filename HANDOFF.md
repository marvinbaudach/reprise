# Startup performance — where this stands

Worktree `~/Projects/reprise-startup-perf`, branch `perf/startup-time`,
`origin/dev` merged three times (last: `d02ec19542`).
Design and measurements: `docs/plans/startup-time.md`.

## Result so far

Medians over six interleaved runs against a copy of the real 242 MB library:

| | before | now |
| --- | ---: | ---: |
| first frame after `present()` | 4053 ms [3065–4890] | **536 ms** [488–670] |
| total to main loop idle | 6499 ms [4936–7956] | **1794 ms** [1653–2638] |
| track-list loads during startup | 6 | 2 |
| sidebar rebuilds | 2 | 1 |

## What landed

- **C0** — `REPRISE_PERF_STARTUP_REPORT=/path.json` writes per-phase offsets and
  counters. Everything below was judged against it. Off unless set.
- **A** — one library load instead of six, one sidebar rebuild instead of two.
- **B** — image decode/scale moved off the main thread; a startup-quiet gate
  (`ui::startup_quiet`) holds back podcast artwork, radio now-playing,
  MusicBrainz and the spectrogram batch until after the first frame.
  Follow-up fix: the gate released everything at once and overflowed the
  artwork queue, dropping 29 images — repaired, now zero overflows.
- **Library Doctor** — `revert_available()` no longer computes a change count
  nobody reads (~800 ms of `SCAN tag_write_journal`); page construction moved
  behind the gate, still openable immediately via `Deferred::get`.
- **Due-check register** (`reprise_core::library::startup_tasks`) — the library
  scan is skipped when the previous session ended cleanly < 15 min ago; the
  spectrogram and cover passes are skipped when the library signature is
  unchanged. Every skip is logged with its reason.
- **Composition root** back under its 600-line limit (`window.rs` 592).

## Verified by hand, not just by tests

- 4897 tests pass, 0 fail; clippy clean; `scripts/check-architecture.sh` passes.
- Two launches within 15 min: second one does zero library scans and zero
  analysis passes, and says why in the log.
- Hard kill then launch: the scan runs. Absence of a clean-exit marker means
  scan, never the reverse.
- Screenshots: track list restores selection, sort, covers and playing marker;
  podcast page shows all three artworks; zero queue overflows, zero decode
  errors.

## Open

1. **Needs verification — the only thing between here and a merge.** Codex
   committed the missing-files change as `88430e75d0` ("perf: time-window
   filesystem-dependent startup checks"). Nobody has built or run it yet. Do:

   ```
   cargo build --release -p reprise-gnome
   cp target/release/reprise ~/.cache/reprise-startup-bench/reprise-miss
   cd ~/.cache/reprise-startup-bench
   REPRISE_BIN=$PWD/reprise-miss ./two_launches.sh   # 2nd launch: no check, logged reason
   REPRISE_BIN=$PWD/reprise-miss ./kill_test.sh      # after hard kill: check runs
   ```

   The point to confirm: it uses the **time-window** rule, not the library
   signature. "Does this file still exist" changes without the database
   changing, so a signature-based skip would hide deleted files forever. If the
   diff keys the missing-files check off the signature, that is a bug, not a
   nuance.
2. `cargo fmt --check` is red on `origin/dev` itself in
   `crates/reprise-core/src/library/library_doctor/album_grouping_tests.rs`
   (from `ed2ab6ccba`, #382 branch) — not ours, deliberately untouched.
3. Not pursued: `app.register()` costs ~320 ms of D-Bus wait before the
   database is opened, `PreferencesContext::new` ~140 ms, and ~280 ms goes to
   dynamic-linker symbol relocation across 136 shared libraries before the first
   line of our code runs.
4. 17 commits, ~2200 inserted lines — worth a written PR summary.

## The bench

`~/.cache/reprise-startup-bench/`:
`run.sh` (single run), `compare.sh A B rounds` (**interleaved** A/B, medians and
spreads), `sample.sh` (adds `/proc` state + `eu-stack`), `two_launches.sh`,
`kill_test.sh`, `shots.sh`, `podcast_shot.sh`.

Two traps this bench already fell into: a cold `XDG_CACHE_HOME` makes GStreamer
rebuild its registry (~800 ms of pure artifact), and wall-clock startup drifts
±400 ms with the page cache — the same binary measured 1390 ms before a
`cargo build` and 2326 ms after. **Never compare sequentially; always
interleave.** For anything under ~300 ms the external clock is useless, use the
startup report.
