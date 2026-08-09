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
- **Filesystem-dependent work is time-windowed, not signature-gated**
  (`88430e75d0`). The register has two classes now: `SignatureTask`
  (spectrogram, cover) keeps comparing library revisions, `TimeWindowTask`
  (library scan, **lyrics**) asks how long ago the previous process exited
  cleanly. A library signature cannot reveal a removed sidecar or a deleted
  file, so anything that reads the filesystem belongs in the second class.
  Verified on a real build, not just by tests — see below.

## Verified by hand, not just by tests

- 4897 tests pass, 0 fail; clippy clean; `scripts/check-architecture.sh` passes.
- Two launches within 15 min: second one does zero library scans and zero
  analysis passes, and says why in the log.
- Hard kill then launch: the scan runs. Absence of a clean-exit marker means
  scan, never the reverse.
- Screenshots: track list restores selection, sort, covers and playing marker;
  podcast page shows all three artworks; zero queue overflows, zero decode
  errors.
- `88430e75d0` run on a release build (2026-08-09, binary built from the commit,
  bench against a copy of the real library):

  | launch | clean-exit marker | library scan | lyrics | spectrogram/cover |
  | --- | --- | --- | --- | --- |
  | 1 — fresh config | absent | **ran** | **ran** | ran, recorded signature |
  | 2 — 3 s after a clean exit | present | skipped (`age_seconds=3`) | skipped (`age_seconds=4`) | skipped (signature unchanged) |
  | 3 — after `kill -9` | **absent** | **ran** | **ran** | skipped (signature unchanged) |

  Row 3 is the point: the hard kill brings the filesystem-dependent work back
  while the signature-bound work stays correctly skipped, because the library
  really did not change. `startup_tasks.completed.lyrics` no longer exists in
  the database — the lyrics record left the signature register entirely.

All of this landed on `dev` as `ec58514bf9` (PR #387).

## Open

Three costs were measured and deliberately left alone, in descending order of
what they would buy:

1. `app.register()` waits ~320 ms on D-Bus before the database is even opened.
2. ~280 ms goes to dynamic-linker symbol relocation across 136 shared libraries
   before the first line of our own code runs.
3. `PreferencesContext::new` takes ~140 ms.

None of them is a bug; each needs its own design decision, and none was in this
work's scope.

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
