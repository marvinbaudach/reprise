# Testing strategy and remaining stability work

This document separates mandatory merge evidence from additional work that
would make Reprise more resilient. Detailed release procedures and the full
manual GNOME checklist remain in [RELEASING.md](RELEASING.md).

## Current automated baseline

At refactoring commit `ec95d7e`, the isolated workspace suite contains 1,013
passing tests: 550 in `reprise-core`, 423 in `reprise-gnome`, and 40 in
`reprise-platform-linux`. A further 75 GTK tests are intentionally ignored by
the normal suite because they require a display and process-level GTK
isolation.

The repository also has focused pointer-driven smoke tests, a synchronized-
lyrics smoke, release/package validation, an architecture/frontend linter, and
a merge-readiness linter. Headless success proves behavior and widget state;
it does not prove native Wayland rendering, pointer feel, audible output,
desktop media integration, portals, or real hardware.

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
- Add scalability budgets using generated metadata only: startup/query/scroll
  behavior at 10,000 and 100,000 tracks, bounded row-widget/provider counts,
  and bounded queue/cache memory growth.
- Add accessibility assertions for names, roles, keyboard reachability, focus
  order, high-contrast behavior, and reduced-motion behavior on the principal
  library, player, preferences, and tag-editor flows.

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
