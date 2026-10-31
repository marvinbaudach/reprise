---
slug: artwork-backfill-stays-quiet
worktree: /home/marvin/Projects/reprise-artwork-backfill-stays-quiet
branch: feature/artwork-backfill-stays-quiet
phase: coded
codex_session:
created: 2026-10-30
---
# The phone stops re-searching artwork on every start

The user's complaint (paraphrased from German): "What bothers me are the constant searches for
images with progress bars — nothing changes that often there." This is about the Android app. The
artwork bar (`ArtistPhotoEdgeProgress`) appears again and again, although the library has not
changed.

Goal: a backfill run that has nothing to ask the network for shows nothing, and repeated
automatic starts don't re-ask the network for the same artists and albums. A bar appears only
for real work, and its count covers only that work.

All facts below were read at `origin/dev` 73e0e3d106. The decisions were grilled with the user on
2026-10-30.

## Diagnosis

The handoff `HANDOFF-2026-10-30-artwork-backfill-quiet.md` blamed the album-cover pass for having
no disk negative cache. **That is wrong for the batch pass.** It goes through
`cover_download::fetch_and_cache_in`, which already has a persistent `<album_key>.notfound3`
marker with a 7-day TTL (`cover_download.rs:25,134,201-233,399-475`). The in-process `ATTEMPTED`
memo (`android-ffi artist_portrait/album_cover.rs:70-72`) serves only the per-track on-demand
fetch `album_cover_fetch_with`.

What actually happens:

1. **The bar exists only while the portrait worklist is non-empty.** `pending_artists`
   (`core artist_portrait/backfill.rs:346-376`) keeps only artists whose `cache::verdict` says
   `needs_fetch`. When the worklist is empty, `finish_without_run` emits nothing
   (`backfill.rs:229-232,600-608`). When it is not empty, the run publishes `Preparing` at once,
   and Kotlin shows the bar immediately (`MobileSurfaceViewModel.kt:189-194`).
2. **Artist-shaped errors are never marked.** The server answered but rejected the request. The
   artist is retried at the head, then at the tail, and then dropped with `failed++` and no
   marker (`backfill.rs:463-478`). On the next run it is pending again, and every such run ends
   with " · N without a photo", held for 10 s.
3. **Misses expire together.** Portrait `.notfound` (`cache.rs:13`) and album `.notfound3`
   (`cover_download.rs:25`) markers live 7 days, and the first run writes most of them at once.
   So about weekly, every miss is asked again in one burst.
4. **The chained cover pass walks every album.** `pending_albums`
   (`core cover_backfill.rs:392-416`) returns all albums, and `total = albums.len()` (`:431`).
   For every album, the FFI fetch closure (`android-ffi artist_portrait.rs:389-409`) first runs
   `cover::resolve_source_with_source`, which reads the representative track's full tag over
   SAF, picture bytes included. Kotlin then shows `total + coversTotal`.
5. **The cover pass runs only as a by-product of portrait churn.** It is chained only on a
   portrait `Complete` with `run_id != 0` (`artist_portrait.rs:346-348`). If all portraits are
   fresh, no cover pass runs, and a new album by a known artist never gets a batch cover.
6. **Triggers.**
   - Every activity create (`MainActivity.kt:236-237`) and every restore (`MainActivity.kt:88`,
     `LibrarySession.kt:120-130`) starts a run.
   - Every completed scan calls `scanCompletedRestartArtwork` unconditionally
     (`AndroidLibrarySessionPort.kt:65-73`, `MobileSurfaceViewModel.kt:368-371`). That includes
     the silent onResume auto-scan, which runs at most once per 5 min, and it clears the user's
     Stop. `ScanSummary { added, updated, errors }` reaches Kotlin but is only logged. In the
     scanner, `updated` counts only changed files; unchanged ones go to `skipped_unchanged`.
   - Every real network return starts a run (`MobileSurfaceViewModel.kt:310-343`).

**Ruled out:**
- Missing sidecars: `store_portrait`, the only writer, writes image and sidecar together.
- A prune mismatch: `prune_except` uses the same `key_for` as `verdict`.

## Do not build

- No new album-cover negative cache: `.notfound3` exists, so reuse it.
- Leave the `ATTEMPTED` memo and `album_cover_fetch_with` alone. They are not part of the bar.
- `ArtistPortraitPrefetch.kt` stays unchanged. It is the per-artist prefetch and does not drive
  the bar.
- The desktop's own code (`reprise-gnome`, `reprise-platform-linux`) does not change. The TTL
  constants are shared with it, and that is intended (D2). Say so in the commit body.

## Decisions (grilled, binding)

**D1 — a retry marker for artist-shaped drops: 24 h.**
- A dropped artist gets `<key>.retry` in the portrait cache dir, written atomically like
  `.notfound` (temp file plus rename, under `CACHE_WRITE_LOCK`).
- `verdict` skips an artist that has a `.retry` younger than 24 h and no fresh positive, and
  does not count it as a miss. NET-9 stays true.
- `store_portrait` and `write_negative` remove the `.retry` for the same key.
- `cache_entry_key` learns the `.retry` suffix, so `prune_except` cleans it up.
- A network-shaped error still writes nothing.
- The desktop's `load_cached*` must ignore `.retry`. Verify it by test or by reading, and
  record which.

**D2 — miss TTL: 30 days plus a spread of 0–7 days, deterministic per key.**
- This applies to the portrait `NEGATIVE_TTL_SECONDS` (`cache.rs:13`) and the album
  `NEGATIVE_MARKER_MAX_AGE` (`cover_download.rs:25`).
- The spread comes from a stable hash of the cache key; reuse the existing `hash_hex` key
  material. It must not use a random value, and it must not change between runs.
- A recorded miss is therefore asked again after 30 to 37 days, and markers written in one run
  expire on different days.
- Both constants are shared with the desktop, and the desktop takes the same change.

**D3 — the cover pass triages before it counts, and remembers local art.**
- `CoverBackfill` (core) takes an injected triage closure. The core module keeps no filesystem
  knowledge of its own. During `prepare`, on its worker thread, the closure classifies every
  album, and only albums that need a request enter the worklist and `total`.
- The FFI triage runs in this order:
  1. A stat-only check of the download cache and markers for `(album_artist, album)` under the
     downloaded dir. The result is no request when any of these holds:
     - `Cached`;
     - a fresh `KnownMissing` (`.notfound3`);
     - a fresh `.local` marker.

     The check needs a new `pub` wrapper over `album_cover_state_at_in` in `cover_download.rs`,
     or a sibling module if that is cleaner. `cover_download.rs` has 81 lines of headroom.
  2. Local art via `resolve_source_with_source` (SAF). If it resolves, write
     `<album_key>.local` (atomic, TTL 30 d plus the D2 spread), and the album needs no request.
  3. Otherwise the album needs a request.
- The `.local` marker is consulted only by the triage. The display path
  (`resolve_source_with_source` for showing a cover) ignores it.
- The existing fetch closure keeps its own checks unchanged as the second line against races.
- Albums are keyed by the DB values `album_artist` and `album`, the same key `network_fetch`
  uses.

**D4 — the cover pass also runs behind a portrait run that had no work.**
- On an empty worklist, `PortraitBackfill` reports one terminal `Complete { total: 0 }` with a
  fresh non-zero `run_id` to its listener, instead of nothing. The FFI is its only consumer;
  the desktop uses no backfill API.
- The FFI chains the cover pass behind it when a tree is configured.
- **Nothing that Kotlin would show reaches the listener until the cover pass has counted real
  work.** The forward closure must not push `Running` while the portrait total is 0 (today it
  pushes `Running` for every chain, `artist_portrait.rs:358-372`). If the cover worklist is also
  empty, the final update is `Complete` with total 0, which Kotlin hides.
- A cover-only run with work shows its progress under the portrait run's non-zero `run_id`.
- A portrait run that had work behaves as today, with the cover pass's `total` now triaged.

**D5 — bar policy (Kotlin).**
- Every run with real work shows the bar: no show delay and no item threshold.
- `PAUSED` ("Waiting for a connection") shows nothing: no bar, no summary suffix, and no "Stop
  artwork download" menu entry, because the entry follows the visible bar. The pass resumes on
  a network return as before, and its bar reappears once it is running again.
- `COMPLETE` with failures keeps " · N without a photo" for 10 s. Thanks to D1 and D2 it counts
  only this run's real misses and drops.

**D6 — scan trigger.**
- A completed scan restarts artwork, and clears a user Stop, only when
  `ScanSummary.added + updated > 0`. A scan that found nothing leaves artwork and the Stop alone.
- App start, restore and the network return stay triggers as they are.
- Plumbing: `AndroidLibrarySessionPort`'s `afterScan` becomes `(ScanSummary) -> Unit`. The VM
  method `scanCompletedRestartArtwork(summary: ScanSummary)` takes the summary, so
  `MainActivity.kt:77` keeps passing a method reference. `MainActivity.kt` is at 781 lines and
  must not grow past 800.

**D6b — no start while offline (grilled after review).**
- `startArtistPhotoBackfillUnlessStopped` and the scan restart check the *current* network
  state first.
- "Online" means a validated physical network: the same predicate as
  `isValidatedPhysicalNetwork` (`NetworkReturn.kt:203`), read from `ConnectivityManager` at call
  time.
- While offline, nothing starts. A start that was skipped because the phone was offline is not
  lost: the existing `NetworkReturnDetector` reports offline → online as a real return, and
  that return starts the pass (`MobileSurfaceViewModel.kt:310-343`).
- Pin it with a test: a cold start while offline produces no start, and the first real return
  produces exactly one.
- Known limit, accepted: if the network drops *during* a run, the cover pass, which does not
  pause on `TransientFailure`, can still sweep its few needs-request albums visibly. Do not
  build a fix for this.

**D7 — UX rule.**
- In `docs/ux-rules.md`, NET-7d keeps its text and its `[active]` becomes
  `[replaced by NET-7e]`.
- NET-7e is inserted directly after it. Use this exact text, followed by the full `*Tests:*`
  list:

```
- **NET-7e** [active] [android] [core] — The cover pass is attempted on every app start,
  including after an update without a scan, once the library folder is registered and the
  phone is online, and after every scan that added or changed tracks; a scan that found
  nothing leaves it alone. A start skipped while offline runs on the next network return. It
  restarts once on every real network return after an offline period, but not on a switch
  between two online networks. It does not restart if the user stopped the download in this
  process; the next scan that added or changed tracks, or the next app start, runs it again. A
  pass asks the network only for artists and albums that need it: one with a fresh photo or
  cover, local art, a recorded miss, or a request rejected within the last day is skipped before
  counting. A recorded miss is asked again after 30 to 37 days, spread per artist or album. A
  pass with nothing to ask for shows nothing; otherwise its progress counts only what it asks
  for. While the pass waits for a connection, it shows nothing.
```

- The 8 `net_7d_*` Kotlin tests are renamed `net_7e_*`, keeping the rest of each name: 6 in
  `NetworkReturnBackgroundArtworkTest.kt` and 2 in `LibraryScreenStateTest.kt`.
- The rule switch, the renames and the last behaviour change land in **one commit**. An
  `[active]` rule requires conforming code and at least one rule-named test, and the
  traceability gate rejects tests that name a replaced ID.
- FB-9 is unchanged.

## Tasks (test-first, in order)

Every task follows the same loop:
1. Write the failing test, run it, and see it fail.
2. Implement the minimum.
3. Run the test again and see it pass.
4. Commit.

**Every commit is green.** A red test is never committed on its own: run it to see it fail, then
commit it together with the code that turns it green.

**Test names.** New tests carry plain names without a rule prefix until the Task 8 commit, which
renames them `net_7e_*` together with the rule switch. The traceability gate rejects a test that
names an ID which does not exist yet.

Three test files are near the 800-line cap, so new tests go into new sibling files (wired with
the existing `#[path = "…"]` pattern):

| File | Lines |
|---|---|
| `core artist_portrait/backfill_tests.rs` | 700 |
| `core cover_download_tests.rs` | 784 |
| `ffi artist_portrait_tests.rs` | 645 |
| `ffi artist_portrait_cover_chain_tests.rs` | 586 |

### Task 1 — regression pin (FFI)
**File:** new `crates/reprise-android-ffi/src/artist_portrait_quiet_tests.rs`.

**Test:** `a_second_start_after_a_settled_run_shows_nothing`.

**Setup:**
- a temp cache root and a configured fake tree;
- portraits: one found, one definitive miss, one artist-shaped drop;
- albums: cached, `.notfound3`-marked, local art, and one that comes back `NotFound`.

**Steps:**
1. Run to the terminal state.
2. Start again at once.

**Assertions:**
- The listener receives no visible update. A visible update is one with `run_id != 0` and not
  (`Complete` with total 0).
- Neither the portrait fetch nor the cover network seam is called.
- No SAF resolve happens for the local-art album, because of its `.local` marker.
- **The snapshot is invisible too.** Kotlin posts `artist_portrait_backfill_progress()` right
  after every start (`MobileSurfaceViewModel.kt:351-358`). Assert that the snapshot is invisible
  immediately after the second start and again after that start settles. Today the cover pass
  keeps its last `{done: N, total: N}` after `finish` (`cover_backfill.rs:469-478`). Once D4 gives
  an empty portrait run a non-zero `run_id`, the merged snapshot would read as a visible
  `Complete` with total N.

Run this test first and see it red. Do not commit it red: it lands with the task that turns it
green, after Tasks 3, 5 and 6.

### Task 2 — worklist logs (core + FFI)
- `pending_artists` logs one `tracing::info!` per run with these counts:
  - total artists;
  - fresh portrait;
  - fresh miss;
  - fresh retry;
  - needs fetch, split into never fetched / stale portrait / expired miss / expired retry.
- The cover triage logs total albums, cached, known missing, local (marker), local (resolved
  now) and needs request.
- Check that `info` passes the FFI `EnvFilter` (`android-ffi logging.rs:42-82`), because these
  lines are the phone-side evidence.

### Task 3 — D1 retry marker (core `artist_portrait/cache.rs`, `backfill.rs`)
New `crates/reprise-core/src/artist_portrait/backfill_retry_tests.rs`:
- a drop writes `.retry`, and the next `pending_artists` skips that artist;
- a `.retry` older than 24 h lets the artist back in;
- a later find, or a miss, removes `.retry`;
- `prune_except` removes the `.retry` of an artist who left the library;
- a network-shaped error writes nothing;
- `a_dropped_artist_gets_no_negative_marker` stays green.

### Task 4 — D2 TTL plus spread (core `cache.rs`, `cover_download.rs`)
- The tests pin all of these:
  - the 30-day base;
  - a spread that is deterministic per key;
  - a spread that stays within `[0, 7 d]`;
  - two keys that get different spreads;
  - a marker that blocks at base plus its spread minus 1 s and no longer blocks at base plus
    its spread plus 1 s.
- Update the existing TTL tests:
  - `cache.rs:267-274`;
  - `cover_download_tests.rs`: the `negative_marker_*` set and
    `stale_album_negative_marker_does_not_block_a_new_search`.
- Put any new tests in a sibling, e.g. `cover_download_ttl_tests.rs`, so
  `cover_download_tests.rs` stays under 800 lines.

### Task 5 — D3 triage and `.local` (core `cover_backfill.rs`, `cover_download.rs` or a sibling, FFI `artist_portrait.rs`)
- Rewrite `albums_with_local_art_are_skipped`, which today pins `{done: 2, total: 2}`, into a
  test that pins `total` = only the albums the triage marks as needing a request.
- New test: an empty triaged worklist publishes nothing visible.
- FFI tests:
  - cached, `.notfound3`-marked and `.local`-marked albums never reach the SAF resolver;
  - a local-art album writes `.local` on its first triage and skips SAF on the second;
  - an expired `.local` triggers SAF again.

### Task 6 — D4 chaining without portrait work (core `backfill.rs`, FFI `artist_portrait.rs`)
- Rewrite `an_empty_worklist_publishes_no_progress` to the new contract: exactly one terminal
  `Complete { total: 0 }` with a non-zero `run_id`, and nothing else.
- New tests in `artist_portrait_quiet_tests.rs`:
  - all portraits fresh and one album needs a request → a visible run with a non-zero
    `run_id` and total 1, which ends `Complete`;
  - all portraits fresh and no album needs a request → nothing visible;
  - an update with `Running` and total 0 never reaches the listener;
  - after any terminal state, `artist_portrait_backfill_progress()` is invisible. A merged
    snapshot must not expose a finished cover run under a non-zero `run_id`. Either the cover
    progress resets to its default when its run ends, or the merge ignores a finished cover run.
    The tests decide which, and the implementation documents the choice at the merge site.
- Every existing test in `artist_portrait_cover_chain_tests.rs` stays green.

### Task 7 — D5 and D6 in Kotlin
**Files:**
- `MobileSurfaceViewModel.kt` (660 lines): the visibility gate, the Stop-entry condition, and
  `scanCompletedRestartArtwork(summary)`.
- `AndroidLibrarySessionPort.kt`: the `afterScan` type.
- `LibraryStatusChrome.kt` or `LibraryFrame.kt`, only if the Stop entry's condition lives
  there.

This list is a starting point, not a fence. Stop only if the contract itself is wrong.

**Tests:**
- In `NetworkReturnBackgroundArtworkTest.kt`:
  - a scan with `added = 0, updated = 0` neither restarts artwork nor clears a Stop;
  - a scan with `added > 0`, and one with `updated > 0`, restarts artwork and clears a Stop;
  - D6b: a cold start while offline starts nothing, and the first real network return starts
    exactly one run.
- In `ArtistPhotoProgressBarTest.kt` or `LibraryStatusChromeTest.kt`:
  - `PAUSED` shows no bar, no summary suffix and no Stop entry;
  - re-pin or retire `waitingUsesIndeterminateEdgeProgress`, and the `PAUSED` case in
    `everyPhaseMapsToItsSummarySuffixWithAndWithoutFailures`.
- `artworkStopActionMatchesTheCancelablePhasesAndHidesItsChrome` follows the new phase set.

### Task 8 — rule and renames (`docs/ux-rules.md`, tests)
In the commit that completes the behaviour:
- switch NET-7d to `[replaced by NET-7e]`;
- add NET-7e (D7 text) with its full test list: the 8 renamed tests plus the new Rust and
  Kotlin tests, each renamed with the `net_7e_` prefix;
- run `scripts/check-ux-traceability.sh`.

Edit only the NET-7d and NET-7e entries. Two sibling branches, `chore/browse-4a-active` and
`feature/android-loudness-target`, edit other sections of this file.

## Verification

**Rust gates** (the plan touches `crates/`), each through `heavy-run heavy --`, with output into
a log file:
- `cargo fmt --check`;
- `cargo clippy --all-targets --workspace -- -D warnings`;
- `cargo test --workspace`;
- `cargo audit`, where RUSTSEC-2024-0436 is the only accepted advisory.

**Core purity:** `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` must
print nothing.

**Android:** run `scripts/check-android-suite.sh` (floor of 334 tests), then
`npm --prefix android run lint`, one Gradle invocation at a time.

**Other gates:**
- `scripts/check-ux-traceability.sh`;
- `scripts/check-architecture.sh`, which covers the panic-lint budgets: no new
  `unwrap`/`expect`/`panic!`.

**Code-file cap:** every code file created or substantially edited ends under 800 lines.

**On the phone** (after the code phase): take the device lock and install a release APK with the
debug-key fallback. `~/.cache/reprise-apk/build-apk.sh` builds `origin/dev` by default; check
whether `REPRISE_APK_WT=<worktree>` points it at the feature worktree. If it does not, run
`scripts/android-build.sh` with `ANDROID_TARGET=aarch64-linux-android ANDROID_ABI=arm64-v8a`,
then `assembleRelease` in the worktree. Afterwards confirm that `dumpsys package` shows no
`DEBUGGABLE`. Then run these
steps and read logcat for the Task 2 lines and `Scan completed: added=… updated=…`:
1. Cold start twice in a row. The second start shows no bar, and its worklist log shows 0
   needs-fetch and 0 needs-request.
2. Leave the app for more than 5 minutes and come back, with no new files. The auto-scan logs
   `added=0 updated=0`, and no bar appears.
3. Add one new album and rescan. A bar appears, and its count covers only that album, plus its
   artist if that artist is new.
4. Airplane mode on, then cold start: no bar and no "Waiting for a connection". Airplane mode
   off: at most one bar, for the pending items only.

**Control arm:** the before-state is the user's report, as decided in the grill. The current
build does not log the worklist.

## Parallelität

**No cut — one strand** (confirmed in the grill).

The reason is the rule coupling. NET-7e replaces NET-7d in one commit, together with conforming
Rust code and the renamed Kotlin tests:
- an `[active]` rule requires conforming code at the switch;
- the traceability gate rejects tests that name unknown or replaced IDs.

A cut along `crates/**` and `android/**` leaves neither branch able to go green before the
merge, in either order. The Kotlin share is small anyway.

**Ownership:**
- `crates/reprise-core/src/artist_portrait/{cache,backfill,cover_backfill}.rs` and new test
  siblings in that directory;
- `crates/reprise-core/src/cover_download.rs` plus its test files, for the TTL, the state
  wrapper and the `.local` marker only (or a new sibling module for them);
- `crates/reprise-android-ffi/src/artist_portrait.rs` and the new
  `artist_portrait_quiet_tests.rs`; `artist_portrait/album_cover.rs` is read-only unless a
  shared helper is needed;
- `android/app/src/main/java/io/github/marvinbaudach/reprise/{MobileSurfaceViewModel,AndroidLibrarySessionPort}.kt`,
  plus `LibraryStatusChrome.kt`/`LibraryFrame.kt` for the Stop-entry condition only;
- the Kotlin tests named above;
- `docs/ux-rules.md`, the NET-7d and NET-7e entries only.

**Post-merge cross-checks:** none beyond the phone verification. A single strand has no seam.
