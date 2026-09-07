---
slug: refactoring-survey-2026-09-07
phase: findings
created: 2026-09-07
base: fe89dc51ad (origin/dev)
branch: chore/cleanup-2026-09-07
---
# Refactoring survey — what is left, what was done, what is not worth doing

Measured against `origin/dev` @ `fe89dc51ad` in a clean worktree, not against a
feature branch. Every number here was counted in the tree, not quoted from an
older document.

## 0. Verdict

The architecture is in better shape than the open documents suggest. The July
review (`docs/plans/architecture-consolidation.md`) has largely been executed:
the second runtime was shelved and deleted (ADR 003), diagnostics and the panic
hook landed, the schema moved from 50 to 83. What remains is not one large
consolidation but a short list of specific things, and the largest of them is a
user-visible bug rather than a structural one.

The most valuable finding of this survey is a **negative** one, in §4.

## 1. Landed in this pass

Branch `chore/cleanup-2026-09-07`, seven commits. Verified with `cargo clippy
--workspace --all-targets -- -D warnings` clean, 2,946 Rust tests, the full
Android gate (99 suites, 605 tests), plus `check-architecture.sh`,
`check-frontend-thinness.sh`, `check-shell.sh` and `check-project-quality.sh`
— all green.

Three reviewers went over the diff afterwards and found four things, all now
fixed. The one that mattered: the first version of the duration fix carried a
doc comment claiming "a change to one contract fails the other side's test",
and nothing made that true — two hand-copied tables of numbers is a plea, not a
mechanism. `scripts/check-duration-format-parity.sh` now is the mechanism; it
reads the assertions out of both files and fails the architecture gate on
divergence, and it was proven to fall before it was trusted. The review also
caught a fresh wrong claim I had put into `AGENTS.md` about which crate
produces the analysis sidecars — in the very file being corrected for wrong
claims. Also fixed.

- **A real Android bug.** `formatDuration` never emitted an hour component, so
  anything past the hour wrapped into the minute field. `BrowseTabs` formats a
  whole album's total with it: a 74-minute album read `74:00`, a 1:02:33
  podcast episode read `62:33`. The rule already existed correctly in Rust as
  `reprise_core::format::format_duration`. The Kotlin copy stays deliberately —
  an FFI hop per row per frame would cost more than the duplication — and a
  parity gate now holds the two to the same cases.
- **A second, quieter bug in the same function.** It formatted through the
  default locale. Measured: under `ar-EG-u-nu-arab` the old body rendered
  `3:01` as `٣:٠١` while the desktop showed ASCII for the same track.
  `SleepTimerControl` already used `Locale.ROOT`; `formatDuration` was the
  outlier.
- **A gate that silently skipped a shipped crate.** `bump-version.sh --base`
  had no case arm for `crates/reprise-cli/*`, so a change confined to it
  printed "no desktop or Android app changes" and bumped nothing — while Meson
  installs that crate's worker binary under `libexecdir`. The same case still
  carried arms for `reprise-runtime` and `reprise-runtime-client`, deleted by
  ADR 003.
- **Five duplicated decisions collapsed.** `fnv1a_64` in
  `podcasts/downloads.rs` (this hash names directories on disk — drift orphans
  downloaded files); `BUS_NAME`, `OBJECT_PATH` and the absent-player
  classification that `reprise-mcp/src/device_sync.rs` had rewritten beside
  `playback.rs` **in the same crate**, with no comment marking it as a copy;
  and a bare `500` standing next to `data::MAX_TRACK_IDS`.
- **Eleven unreachable `pub` items removed.** A `pub` item inside a `pub mod`
  chain is a reachability root, so the dead-code lint never fires on it. Each
  was traced to zero call sites across `crates/`, `android/`, `scripts/`,
  `acceptance/` and `quality/` first.
- **`AGENTS.md` made true again.** Its crate list described `reprise-runtime`
  and `reprise-runtime-client`, both deleted, and never mentioned
  `reprise-view` or `reprise-android-ffi` — the two crates that carry the
  multi-frontend work. The count "nine" stayed right while two of the nine were
  fiction. This is the first file an agent reads here.

## 2. The largest thing still open: Android reads block the main thread

Highest payoff of anything found, and the only item with a symptom a user
feels. `BrowseScreen.kt`'s background prefetch wraps its FFI query in
`withContext(Dispatchers.IO)` with a comment saying exactly why — "the rows
come off a blocking JNI + SQLite call". The pagination lambdas directly below
it call the *same* functions with no such wrapper, driven by `LaunchedEffect`,
which runs on the main dispatcher. The whole file contains two `withContext`
occurrences.

Affected: `loadMoreTitles`, `loadMoreArtists`, `loadMoreAlbumTracks`,
`loadMoreArtistTracks`, `loadMoreArtistAlbums`, `search()` (the as-you-type
filter) and `openAlbumDetail`. That is scroll-to-load-more, type-to-filter and
tap-an-album — the three most-touched interactions in the app.

The cause is one level up: `MainActivitySurfaceDependencies` declares its query
fields as plain value-returning functions rather than `suspend`, so every call
site must remember to dispatch, and most did not. Making that seam `suspend`
fixes the class rather than the instances, and it is also the honest answer to
the old "no repository/ViewModel layer" finding — the payoff there was never a
shorter parameter list, it is one place that makes every read `suspend`.

There is a working reference implementation to copy in the same file.

## 3. Smaller, still real

- **`scan_folder_inner` is 433 lines** (`library/scanner.rs:266-698`) against a
  house rule of under 50. `scanner_vanish.rs` says in its own module doc that it
  exists "purely to keep `scanner.rs` under the project's 800-line rule" and
  imports its parent with `use super::*`. The file split is the symptom; the
  function is the cause. Repo-wide, 53 production files cite the 800-line rule
  as their reason for existing — the rest of the sample split along real
  responsibility seams, so this one is the outlier, not the pattern.
- **`reprise-android-ffi` has no size budget** while `reprise-gnome` has
  several. It has grown from 7,970 to 9,089 non-test lines. Separately,
  `artist_portrait.rs` sits at 793 lines against the hard 800-line ceiling —
  seven lines of headroom before an unrelated commit trips the gate.
- **Seven hand-built single-thread executors** on Android instead of
  coroutines, in `TrackAnalysisLoader`, `TrackCover` (two), `LibraryWrites`,
  `ActivityPlaybackControls`, `TrackLoader` and `ArtistPortraitPrefetch`. Each
  is coherent and individually well-named with working interrupt handling, so
  this is debt rather than a bug. Lower priority than §2.
- **`rusqlite::Error` is still the core's public error type**, in roughly 705
  places, and no `CoreError` exists. It leaks into `reprise-gnome`,
  `reprise-cli` and `reprise-mcp`; `reprise-view` and `reprise-android-ffi` are
  already clean. This is Wave 3 of the old plan and remains correctly scoped
  there — it is a project, not a cleanup.

## 4. Decisions still written twice — and the one that fails silently

The recorded lesson is that a comment asking two places to stay in sync is a
plea, not a mechanism. Three instances remain, deliberately not fixed here
because each needs a decision about where the shared thing lives:

1. **Listen-report filenames, Rust to Kotlin.**
   `device_sync/listen_report.rs` and `ListenReportWriter.kt` each spell
   `reprise-listens-back.rpl` themselves. No compiler, no test, and not even a
   comment. A rename on either side breaks the phone-to-desktop handshake
   **silently** — the file is simply never found. The most dangerous of the
   three. Either export the constants through the FFI, or do what §1 did for
   durations: one table of shared test vectors asserted on both sides.
2. **`is_absent_player` in `reprise-cli` and `reprise-mcp`.** Neither may
   depend on `reprise-platform-linux`, where the server holds the truth, which
   is why it was copied. Needs a neutral home.
   The pattern to copy for case 1 is now in the tree:
   `scripts/check-duration-format-parity.sh` holds two hand-written copies of
   one rule together by comparing what each side asserts, without an FFI hop.
3. **`BUS_NAME`/`OBJECT_PATH` in four files.** Fails loudly rather than
   silently, so last.

## 5. What is NOT worth doing — the useful negative

**The `sources_http` consolidation is much smaller than its note claims.** That
note says `podcasts/http.rs`, `radio/http.rs` and `concerts/http.rs` "clone the
same HTTP boundary idiom" and asks that the task not be allowed to evaporate.
Measured: of 1,297 combined lines, roughly 90 would collapse.

What is genuinely identical is `user_agent()`, `lock_unpoisoned()`, the ureq
client-builder chain and the fixture scaffolding. What looks duplicated but is
not: the rate limiter (concerts can cancel mid-wait, the other two cannot), the
status-code mapping (only podcasts distinguishes source-gone), and the fixture
route matching (different hosts, paths and query keys per source). The three
error types are unrelated — `ProviderError::Transport` is a unit variant while
the others carry a string — so a shared classifier would have to become generic:
more machinery than saving.

`fnv1a_64`, the other half of that note, is now finished: five definitions in
August, four already consolidated, the last one in this pass.

**Recommendation:** do not plan this as a consolidation task. Take the four
identical pieces along whenever one of the three files is touched anyway.

**Also not worth doing:** merging `column_header_dnd.rs` (5 lines) and
`column_layout_editor.rs` (16 lines). Both export `css()`, so merging forces a
rename and changes call sites in the style aggregator — churn exceeding the
gain. The file-size distribution as a whole is healthy: most splits land on real
seams and re-export transparently.

## 6. Suggested order

1. §2, the Android main-thread reads. Only user-visible item, and the seam
   change closes the class.
2. §4.1, the listen-report filenames. Cheapest of the three drift risks and the
   only one that fails silently.
3. §3, the `reprise-android-ffi` budget — script-only, and `artist_portrait.rs`
   will otherwise trip the ceiling on an unrelated commit.
4. §3, decompose `scan_folder_inner`. Highest-risk file in the app; worth doing
   only with the scanner's own tests as the net.
