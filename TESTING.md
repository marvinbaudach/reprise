# Testing strategy and remaining stability work

This document separates mandatory merge evidence from additional work that
would make Reprise more resilient. Detailed release procedures and the full
manual GNOME checklist remain in [RELEASING.md](RELEASING.md).

## Current automated baseline

At performance close-out commit `a41c53f`, the isolated workspace suite
contains 1,482 passing tests: 758 in `reprise-core`, 669 in `reprise-gnome`,
and 55 in `reprise-platform-linux`. Another 139 tests are deliberately
separated from the default run: one ignored core probe and 138 GNOME tests
whose display or host contracts require controlled execution.

The repository also has focused pointer-driven smoke tests, a synchronized-
lyrics smoke, release/package validation, an architecture/frontend linter, and
a merge-readiness linter. The QA policy also executes
`scripts/tests/readme-showcase.sh`, which keeps the English and German showroom
documents aligned on their date, evidence, architecture, and roadmap status.
Headless success proves behavior and widget state;
it does not prove native Wayland rendering, pointer feel, audible output,
desktop media integration, portals, or real hardware.

### Local audio-analysis evidence

Audio-character tests use only generated signals and small redistributable
fixtures committed with their origin and license. Automated runs must never
open the maintainer's music library. Synthetic silence, tones, click tracks,
dynamics, and noise provide independent expected orderings; real codec
fixtures prove decoding compatibility without serving as subjective mood
ground truth.

Before shipping local audio analysis, retain a release-profile report for peak
RSS or a directly proven PCM-buffer bound, decode time per audio minute,
database bytes per 10,000 tracks, the pending query at 100,000 tracks, and
deterministic profile output across chunk boundaries. Elapsed values are
same-host evidence rather than portable promises. The hard contracts are
bounded streaming memory, one default analysis worker, versioned results, and
no network or write access to source audio.

### Mix-planning and agent safety matrix

Mix-planner tests use generated metadata and in-memory databases only. They
cover malformed and unknown intent fields, non-finite and out-of-range profile
values, oversized explicit ID lists, contradictory sources and exclusions,
stale analysis, missing or removed tracks, the 500-candidate ceiling, stable
tie-breaking, duration underfill, and draft expiry. Preview, playback, queue,
and playlist approval must all consume the same persisted draft positions;
tests fail if a caller can submit replacement track IDs during approval.

Related-artist providers are system boundaries. Automated tests inject fixture
responses and prove opt-in gating, bounded requests, cache expiry, provider
attribution, canonical in-library exclusion, and hide/restore behavior without
contacting a live service. A provider failure may remove discovery suggestions
but may never prevent a local mix based on available evidence.

Future MCP adapters must repeat the validation matrix at their schema boundary:
no free SQL or file paths, bounded pagination and ID counts, read/plan/create
capabilities separated fail-closed, draft planning unable to mutate playback or
the queue, and playlist creation idempotent and limited to an approved draft.

### Generated-metadata scalability baseline

Run the release-profile scalability baseline with an explicit new output
directory:

```sh
scripts/performance-baseline.sh /tmp/reprise-performance-results
```

The normal run generates fresh 10,000- and 100,000-track databases under a
private temporary directory, measures database open/migration, library count,
first/middle/final 200-row windows, filtered count, library statistics, and
playback-id projection. It then measures committed batches of up to 10,000
inserts, index-relevant metadata updates, present-to-missing transitions, and
missing-to-present restores before exercising TrackListModel scroll access at
the same sizes. Every write sample uses a disposable copy of the generated
database, so iterations start from identical state and never mutate the
source profile used by the read measurements. The query JSON also records
SQLite's title-window query-plan details, whether the plan needs a temporary
ORDER BY sort, and the selected index name. It retains a manifest, stable-schema
query JSON, and model logs in the requested output directory. `--quick` runs
only the 10,000-track scenario.

Elapsed times are evidence for comparing two commits on the same host, not a
portable CI threshold. Deterministic budgets are hard assertions: the model may
retain at most eight SQL windows and 1,600 track rows regardless of library
size. The runner requires a clean Git worktree so its manifest identifies the
compiled sources exactly, refuses an existing output directory, and the core
probe refuses an existing database, so neither can overwrite a user profile.
All rows are generated metadata with synthetic paths; no audio file is opened.

Compare two generated-metadata runs after changing query or database code:

```sh
scripts/performance-query-compare.sh /tmp/before /tmp/after \
  > /tmp/query-comparison.json
```

The report includes database size and open-time costs, first/middle/final
window and playback-id timing deltas, committed insert/metadata-update/hide/
restore batch deltas, and the before/after SQLite query plans. It rejects
different write-batch sizes rather than comparing unlike workloads. This makes
an index tradeoff visible even when private display sockets are not available
for the installed-runtime benchmark.

For the installed-runtime extension, use a second new output directory:

```sh
scripts/performance-runtime-baseline.sh /tmp/reprise-runtime-results
```

This runner builds a release Meson installation in a private `DESTDIR`, seeds
isolated 10,000- and 100,000-track profiles, and launches that installed binary
five times per size. It records process-spawn-to-accessible-window timings,
realized GTK row/cell and provider/model counts, five fresh-process queue RSS
samples, and CUA page-scroll-to-changed-snapshot timings with before/after
screenshots. Deterministic limits reject more than eight cached SQL windows,
1,600 cached tracks, 128 realized rows, 2,048 realized cells, or queue RSS
growth above the documented fixed-plus-linear budget. `--quick` uses 10,000
tracks only.

The CUA portion requires a host that permits a private D-Bus, AT-SPI, and Xvfb
socket. A managed sandbox that rejects those sockets is an environment
blocker, not a passing or failing app result; the runner fails fast and retains
bounded diagnostics. It never falls back to the live desktop or normal XDG
profile.

Compare two complete runs after changing code:

```sh
scripts/performance-compare.sh /tmp/before /tmp/after > /tmp/comparison.json
```

The comparison requires identical track-count manifests and reports exact and
percentage deltas for installed startup, the final sorted SQL window, queue
RSS, observable scroll response, realized GTK rows/cells, and cached tracks.
Negative timing and memory deltas are improvements. Compare the same build and
host conditions; these values are diagnostic evidence, not portable CI timing
thresholds.

## Required merge gates

Every branch intended for `main` must pass `scripts/check-merge-readiness.sh`.
That requires a clean worktree containing the latest `origin/main`, a clean
branch diff, architecture and frontend policy compliance, formatting, Clippy
and Rustdoc with warnings denied, the isolated workspace tests, and
`cargo audit` with no advisory beyond the explicitly accepted `paste`
maintenance warning.

Merge readiness also runs `scripts/check-ux-traceability.sh`: every `[aktiv]`
rule in `docs/ux-rules.md` needs a rule-named test, no test may reference an
unknown or replaced rule ID, no `[aktiv]` rule test may be `#[ignore]`d, and
every `#[ignore]` on a rule-named test must read `UX <ID> [geplant] — …`.
Only real `#[test]` fns and executed cua-e2e lines count as coverage — a
same-named helper fn or a comment does not.

For release candidates, also run:

```sh
scripts/check-release.sh
scripts/check-display-tests.sh
scripts/ptr-e2e/run.sh
scripts/cua-e2e/run.sh
scripts/check-lyrics-smoke.sh
```

The release checker currently inherits a translation-catalog mismatch from
`main`; reconcile the generated POT with `po/de.po` before treating that check
as a release-green signal. Do not weaken `msgcmp` to hide the mismatch.

## Priority automation gaps

### P0 — close before a release candidate

- Run all 75 ignored GTK tests in CI through
  `scripts/check-display-tests.sh`, preferably sharded while keeping one exact
  test per process. They are not part of the fast pre-push hook today.
- Add an isolated installed-app smoke for both a populated and an empty
  library. Assert clean startup/shutdown logs, no GTK/GLib criticals, no panic,
  and no access outside temporary XDG roots.
- Add fault-injection sequences for playback: failed URI, decoder error,
  gapless handoff, crossfade promotion, seek during transition, and queue
  mutation during an error. Assert one state transition and one user-visible
  error per fault.
- Add session-restore round trips covering source, search, browse filters,
  sort, queue order, current track, repeat, and shuffle together. Existing
  unit tests cover pieces but not the complete persisted state projection.
- Make the gettext catalog complete again and keep extraction coverage in the
  merge gate once the inherited baseline is repaired.

### P1 — high-value hardening

- Add property-based operation sequences for queue and playlist mutation:
  insert, remove, reorder, deduplicate, purge missing ids, and persistence.
  Check ordering, bounds, foreign keys, and gapless-current-track invariants.
- Add batch-tag fixtures with mixed values, partial writes, malformed tags,
  read-only files, and one failure midway through a batch. Assert unchanged
  fields and copied audio bytes remain intact.
- Add deterministic race tests for recycled cover rows and stale async results
  from cover, lyrics, and artist-news workers. Generation tokens must reject
  every late result after selection or row identity changes.
- Add database upgrade fixtures for every supported schema version plus a
  failed migration step. Verify rollback, data preservation, indexes, foreign
  keys, and an idempotent second open.
- Run the installed-runtime scalability benchmark on representative native
  GNOME/Wayland release hardware in addition to its reproducible private-X11
  path, and retain paired before/after artifacts for each accepted optimization.
- Add scalability budgets using generated metadata only: startup/query/scroll
  behavior at 10,000 and 100,000 tracks, bounded row-widget/provider counts,
  and bounded queue/cache memory growth.
- Extend the accessibility sweep with real High Contrast, Large Text, Orca,
  on-screen-keyboard, and reduced-motion evidence; these remain host-manual
  because a headless semantic tree cannot prove visible focus or speech output.

### P2 — useful regression depth

- Add fuzz targets for M3U, Rhythmbox XML, LRC, imported JSON, and persisted
  settings parsers. Malformed input must return an error or safe fallback,
  never panic or allocate without a bound.
- Add screenshot-diff coverage for stable, theme-independent geometry only.
  Keep color/font rendering review manual because host themes and renderers
  make pixel baselines brittle.
- Add cancellation and shutdown tests for every background worker so no task
  can update a disposed widget or keep the process alive.

## Isolated GTK and desktop tests

GTK can be initialized from only one thread per test process. Never run the 75
ignored display tests as one filtered Rust test invocation: even
`--test-threads=1` may execute successive tests on different harness threads.
Use `scripts/check-display-tests.sh`, which discovers the tests and launches
each exact test in its own process.

Every headless GTK command must use a private D-Bus session, Xvfb, temporary
`XDG_DATA_HOME` and `XDG_CACHE_HOME`, forced X11, unset Wayland display, and
`REPRISE_AUDIO_SINK=fakesink`. Pointer-driven workflows belong in
`scripts/ptr-e2e`; fixture-only network workflows need local servers and
request logs that assert no path, credential, or listening-history leakage.
Semantic accessibility workflows belong in `scripts/cua-e2e`; its helper
contract rejects unbracketed actions, degraded AT-SPI trees, and suspected
no-ops before the workflow can claim success.

Useful additions to the display suite are recycled Artist/Album rows after
rapid scrolling, live theme changes while transient dialogs are open, compact
mode restore across multiple monitors, and scan/cover progress transitions in
empty and populated libraries.

## Manual release checks

The following evidence cannot be replaced by headless tests:

- real GNOME Wayland rendering at supported scale factors, pointer gestures,
  keyboard navigation, focus visibility, high contrast, and reduced motion;
- audible GStreamer playback across representative codecs, real seeking,
  equalizer/ReplayGain, gapless playback, crossfade, and device changes;
- MPRIS quick settings, media keys, notifications, lock screen metadata and
  cover art, suspend/resume, and clean desktop shutdown;
- host and Flatpak portals for folder selection, opening folders, Trash, and
  keyring access, always using copied disposable files and accounts;
- real MusicBrainz/Cover Art Archive/LRCLIB behavior, offline cache fallback,
  rate limiting, privacy of request fields, and failure messages;
- Android MTP discovery, safe reconnect, capacity warnings, cancellation,
  playlist ordering, and proof that writes stay below `Music/Reprise`;
- first-run and Rhythmbox import against a disposable profile, plus upgrade
  from the previous released build and two subsequent restarts.

Record OS, GNOME, runtime, architecture, codec packages, test fixtures, and
results in the release notes. Never use the maintainer's real music library,
database, credentials, production accounts, or desktop session for automation.

## Known harness constraints

- The normal workspace suite excludes 75 display tests; a green
  `cargo test --workspace` is therefore necessary but not sufficient for GUI
  readiness.
- Xvfb proves widget construction, signals, state and CSS parsing, but not
  final native rendering, GPU behavior, pointer feel, media keys, or portals.
- Multiple GTK tests in one Rust test process can abort before assertions due
  to GTK thread ownership. Use one exact test per process.
- Network tests must use local fixture servers unless a manual release step
  explicitly authorizes a real service and disposable account.
- Audio tests use `fakesink`; audible behavior and hardware changes remain
  manual.
- `scripts/check-release.sh` is stricter than the merge hook and may require
  host packaging tools. Any skipped external validator must be recorded and
  rerun on the release machine.
