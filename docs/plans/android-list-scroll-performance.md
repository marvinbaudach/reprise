---
slug: android-list-scroll-performance
worktree: /home/marvin/Projects/reprise-android-list-scroll-performance
branch: feature/android-list-scroll-performance
phase: planned
codex_session:
created: 2026-08-13
---
# Android: covers that arrive late, and the list that pays for it

Read against `c0df688a19` (#451) on 2026-08-13; implemented on `origin/dev` at
`39cc58cf8b`. The seven commits between the two touch only playback code — none
of the files named below — so every line number here still holds.
Device of record: Pixel 10 Pro XL, Android 17, `applicationId = org.reprise`, arm64.
Every claim below carries its file and line.

## Rules for the implementer — read first

**Do not touch the device, `adb`, the emulator, or `cua-driver`. Do not install
or launch the app. The measurement runs happen outside this task, by the
operator.** Wave 0 asks you to *write* a measurement script and in-app counters;
it does not ask you to run them, and running them is not part of any gate you
own. A previous Android task ignored this and spent 70 minutes driving an
emulator while changing no files at all.

Your gates are the ones that need no device: the Robolectric unit suite, the
Rust tests, `cargo clippy`, the architecture lint, and the Compose compiler
report. Each is spelled out per wave with the exact command.

`BUILD SUCCESSFUL` is not evidence. Count the result XML and quote suites,
tests and failures — a Gradle build can succeed while running no tests at all.

## The complaint

> "Beim Scrollen sieht man, dass die Covers erst geladen werden. Vielleicht kannst du noch Speicherverbrauch und CPU optimieren. Unnötiges Rendern reduzieren. Windowing, lazy loading."

Two of those are one problem and three are another. The **visible** one is
latency: a row appears, and its cover appears afterwards. Frame time and cover
arrival are separate metrics and the second one is the complaint — no
frame-timing metric can see a cover that is simply not there yet. So cover
arrival comes first and needs a measurement of its own.

A caution about an existing number, because it is easy to misread: a release
build measured 0.20 % janky frames at an 11 ms median on this device on
2026-08-11. **That was the Now Playing scene, not the library list.** It says
nothing about a list whose paging runs a SQLite query on the main thread every
200 rows (below). Do not treat list frame time as known-good; Wave 0 measures
it for the first time.

## What is actually there

### Reading one cover, end to end

`LibraryTrackRows.kt:268` renders `TrackCover(trackUri, size, decorative = true)` per row. That reaches `rememberTrackArtworkVisual` (`TrackCover.kt:237`), which seeds state synchronously and then asks `TrackArtwork.loadVisual` for the real thing. On a cache miss the work is queued onto **one** thread (`TrackCover.kt:41`, `singleArtworkThread("reprise-artwork-list")`) and does, per cover:

1. `session.artworkFor(uri, LIST)` — memoised per process in a 512-entry LRU (`LibrarySession.kt:11,72,196`), so this is paid once per track per **activity instance**. On a miss it calls the FFI.
2. `MusicLibrary::track_artwork` (`crates/reprise-android-ffi/src/lib.rs:237`) → `resolve_source_with_source` (`crates/reprise-core/src/cover.rs:83`) → `read_cover_tag_with_source` → `source.open_read()`, which on Android is a **binder round trip to the DocumentsProvider** (`crates/reprise-android-ffi/src/source.rs:163`) followed by a full lofty tag parse and a `picture.data().to_vec()` copy of the embedded JPEG.
3. `thumbnail_with_source` (`cover.rs:216`) hashes those bytes to build the cache key and only *then* checks whether the PNG already exists. **A warm thumbnail cache saves the resize, never the tag read.** The desktop solved exactly this with the resolution index at `cover.rs:406` — three `stat`s instead of a tag read, with the comment "a 1800-track library read 455 sets of tags on every single launch, cache warm or cold". The Android entry point does not use it: `resolution_index_path` (`cover.rs:349`) is hard-wired to `cache_dir()`, the XDG directory, which does not exist on the device.
4. `BitmapFactory::decodeFile` with no `Options` (`TrackCover.kt:39`) → ARGB_8888, 168 × 168 = 112.9 KB.

So the first pass over a library is one binder round trip plus one full tag parse **per row, serialised on a single thread**. That is the trickle the user sees. The library of record has 2132 tracks across 429 albums — the same order of magnitude as the 1800-track desktop case that motivated the resolution index. Every later pass is decode-only — until the process or the activity is replaced, at which point `LibrarySession` is rebuilt (`MainActivity.kt:65`) and all 512 memos are gone.

### The cache is one shelf for two very different objects

`ArtworkCache.kt` keeps a **single** `visuals` LRU of **12 entries by count** for LIST covers (113 KB), NOW_PLAYING covers (1092² × 4 B = **4.77 MB**) and generated fallbacks alike. `NowPlayingScene.kt:96,103,111` asks for three NOW_PLAYING visuals at once — current, previous and next. Six of those entries are 28 MB and evict every list cover in the cache. This is the mechanism behind "I opened the player, went back, and every cover reloaded". Twelve list covers is also only one screenful: scrolling down and back up re-decodes everything by construction.

Every method is `@Synchronized` on the instance, and the **main thread takes that same lock** on every row through `seedVisual`.

Note what this means for the *shape* of the memory problem. The device has 15.4 GB of RAM and a 256 MB heap growth limit; a 16 MB cover cache is 6 % of that budget. The problem is not the quantity of cover memory. It is that one shelf holds objects that differ by a factor of forty, that nothing bounds it in bytes, and that nothing can reclaim it under pressure.

### The main thread builds a 4.77 MB bitmap during composition

`TrackCover.kt:248` seeds with `artwork?.seedVisual(request)` inside `remember`, i.e. **during composition on the main thread**. `seedVisual` falls through to `generatedVisual`, which on a miss calls `fallbackCoverBitmap` — an FFI call, a `Bitmap.createBitmap`, a `LinearGradient` fill and three shapes (`FallbackCover.kt`).

For the **list** this is much cheaper than it looks: `LibraryTrackRows.kt:268` passes no `title` and no `artist`, so every list row shares the generated key `("", "", LIST)` (`ArtworkCache.kt:98`). After the first row it is a cache hit — two map lookups under the shared lock. The main-thread cost per list row is the lock, not a bitmap.

For **Now Playing** it is the real thing. `NowPlayingSheet.kt:253` and `NowPlayingScene.kt:96/103/111` do pass title and artist, at 1092 px. Three uncached neighbours mean **14.3 MB allocated and three gradient fills on the main thread inside a composition**. That is a visible stall on the play view and a genuine memory spike.

### What recomposes, and why

`Media3PlaybackPort.kt:24` pushes a snapshot every **500 ms** while playing; `MainActivity.kt:157` writes it into one `mutableStateOf(PlaybackUiState())`. `TrackRows` takes that whole object and each item lambda reads it via `content.track.playbackPresentation(playback)` (`LibraryTrackRows.kt:201`). `positionMs` changes twice a second, so **every visible row recomposes twice a second during playback**.

The work is provably wasted: `playbackPresentation` (`LibraryFramePolicy.kt:44–52`) reads exactly `playback.currentTrackId` and `playback.isPlaying` and nothing else. It never reads `positionMs`. So the derived value is bit-for-bit identical across a tick, and no row's appearance depends on the position at all.

Rows cannot skip that work even in principle, because `TrackListItem` and `LibraryTrackRow` take `surfaceState: MobileSurfaceViewModel` — a class with public `var`s and an external supertype, i.e. **unstable** to the Compose compiler, which makes the composables non-skippable. Same for `LibraryWindow<T>`, whose `rows: List<T>` is an unstable type.

Two smaller ones in the same area:

- `Modifier.trackContextMenuAnchor` (`TrackContextMenu.kt:78`) is built with `composed { }`, the factory that has no equality and therefore defeats modifier skipping and reuse. Every row uses it.
- `ObserveLibraryListAnchor` (`LibraryListAnchor.kt:117`) runs `snapshotFlow { state.anchor() }` which allocates a `LibraryScrollPosition` and does a float division **every scroll frame**; `distinctUntilChanged` cannot help, because the offset genuinely changes each frame.

Two premises that do **not** hold, so nobody spends a step on them: `LibraryTrack`, `LibraryAlbum` and `LibraryArtist` are `data class`es of `Long`/`String`/`Int` in the app module — the compiler already infers them stable, and `@Immutable` would change nothing. And the missing `key` in `AppearanceSettingsPage.kt:40` / `PlaybackSettingsScreen.kt:150` is irrelevant: those are static lists of a handful of items that no one flings. Fix them if you are passing by; do not plan around them.

### `contentType` is not missing so much as wrong

No list sets `contentType` anywhere. The default is `null` for *every* item, which does not disable reuse — it puts all items in **one** reuse bucket. In the homogeneous lists that is harmless. In `BrowseTabs.kt:339` and `:408` it is not: section headings, album rows, artist rows, track rows and the "Loading…" sentinel all share one bucket, so a slot laid out for a heading gets handed to a track row and its whole subtree is thrown away. That is where `contentType` earns its keep.

### Paging runs a SQLite query on the main thread

The continuation sentinel is an ordinary list item (`LibraryTrackRows.kt:206`): `LaunchedEffect(offset) { loadMore(request) }`. `loadMore` is `loadMoreTitles` (`BrowseScreen.kt:333`), which calls `searchTitles(...)` **synchronously**. A `LaunchedEffect` body runs on `AndroidUiDispatcher.Main`, so paging in the next 200 rows performs an FFI call that takes the library mutex and runs a SQLite query **on the main thread** — the same mutex `MusicLibrary::scan` holds for an entire SAF folder walk. Every 200-row boundary during a fling is a multi-frame stall, and it is the only structural jank source found in the list.

The repo already has the answer to this, twice: `TrackLoader.kt` and `PlaybackQueryRunner` (`ActivityPlaybackControls.kt:20`), both built for precisely this lock. `BrowseScreen.kt:414` even carries the comment explaining why the playing row is read that way.

And because the sentinel only loads when it is *composed*, the listener always sees the "Loading…" row: there is no look-ahead at all. `TrackArtwork.prefetch` exists and is fully built, but its only caller is the playback queue (`MobileSurfaceViewModel.kt:187` via `BrowseScreen.kt:424`). Nothing prefetches for scrolling.

## Target picture

A fling shows covers that are already there for the rows on screen and for the next screenful. Nothing on the main thread opens a file, queries a database or allocates a megabyte-scale bitmap. Cover memory has a budget in bytes that the process respects and gives back under pressure. Rows recompose when their own track or their own rating changes, and not when the play position ticks. Paging happens before the listener reaches the end of what is loaded, off the main thread. And all of that is stated in numbers produced by a procedure anyone can repeat.

---

# Decisions already taken

These were settled with the user before implementation began. They are not open;
do not reopen them, and do not implement an alternative "while you are there".

1. **Measurement is a shell script plus in-app counters. No macrobenchmark
   module.** `androidx.benchmark.macro` measures frames, and frames are not the
   complaint; seeing cover latency would mean adding `Trace` sections to the app
   anyway. It also needs a second Gradle module and a connected device for every
   run — a gate the headless implementer can never execute — and its payoff is CI
   regression tracking, which this repo has no Android CI to collect. If that
   changes, the module is a separate plan and consumes the `Trace` sections this
   one adds.
2. **A2 passes the stamp in rather than reading it.** `resolution_stamp`
   (`cover.rs:377`) takes the stamp as an argument instead of calling
   `std::fs::metadata` itself. The desktop supplies it from the filesystem as
   today; Android supplies it from `tracks.file_mtime` and `tracks.file_size`,
   which are already columns on the row `track_artwork` reads anyway (there is
   even an existing trigger, `invalidate_track_render_data`, that invalidates
   derived data on exactly those two fields). **No schema migration, no extra
   round trip.** The signature change recompiles the desktop path — that is the
   accepted cost, and the desktop's own tests are the guard.
3. **List covers stay ARGB_8888.** Memory is bounded by the byte budget, not by
   bit depth: 100 list covers is 11 MB against a 256 MB heap limit. RGB_565 would
   trade image quality for memory that is not scarce; `HARDWARE` was rejected for
   the Robolectric risk. (For the record, `HARDWARE` is impossible for
   NOW_PLAYING regardless — `AmbientArtwork.kt:68`, `CoverShadowBitmap.kt:95` and
   `CoverFogBitmap.kt:149,188,212` all read pixels back.)
4. **Cover budget: 16 MB for the LIST shelf, a separate shelf sized for three
   NOW_PLAYING covers, and a small shelf of its own for generated fallbacks.**
   Wave C tunes these with numbers; they are starting values, not guesses to
   defend.
5. **120 Hz for lists is built in Wave B but only enabled if the measurement
   earns it.** At 120 Hz the main thread has 8.3 ms per frame instead of 16.7.
   Requesting it before the list is fast enough converts clean 60 Hz frames into
   dropped 120 Hz ones. Build it behind the existing pattern; Wave C decides.
6. **The window stays at 200 rows** until Wave C has numbers.
7. **The list's generated cover stays neutral.** `LibraryTrackRows.kt:268`
   deliberately passes no title or artist, so every coverless row shares one
   cached 113 KB bitmap. Passing them per row would allocate one bitmap per
   coverless track. Do not "fix" this.

---

# Measurement discipline

Three rules, each bought with a wasted afternoon. They bind the operator's runs,
and they are recorded here because the script must encode them:

- **Never take a frame time from a debug build.** The same scene measured 97.7 % jank / 18 ms median in debug and 0.20 % / 11 ms in release on 2026-08-11. Debug numbers are noise about the compiler.
- **`dumpsys gfxinfo org.reprise reset` runs immediately before the swipes**, with the app already launched, warmed and scrolled once. A window that contains the app start reported a 93 ms median instead of 11 ms.
- **No per-frame `Log.d`.** A logging probe on the frame path is part of what it measures. Counters aggregate in memory and are read once, on demand.

A reproducible fling is the precondition for everything: without identical input, before and after cannot be compared, and every number is decoration.

---

# Packages and waves

Waves are barriers. Inside a wave, packages own disjoint files so two agents never write the same file. **Ownership is a write-conflict boundary, not a fence.** A package may and should change whatever adjacent code inside its own area it needs — a signature, a call site, a test, a helper it has to extract — and name it in the commit message. What it must not do is edit a file another package owns *in the same wave*; if it needs one, it says so and the change waits for the next wave. Stop only if the *contract* is wrong.

| Wave | Package | Owns |
|---|---|---|
| 0 | P0 | `scripts/android-scroll-baseline.sh` (new), a new artwork-telemetry file, the `dump` hook in `MainActivity.kt`, `android/app/build.gradle.kts` |
| A | A1 | `TrackCover.kt`, `ArtworkCache.kt`, `FallbackCover.kt`, `ArtworkRequestGate.kt` and their tests |
| A | A2 | `crates/reprise-core/src/cover.rs` (+ its test files), `crates/reprise-android-ffi/src/lib.rs`, `crates/reprise-android-ffi/src/artwork_tests.rs` |
| A | A3 | `LibraryTrackRows.kt`, `LibraryFramePolicy.kt`, `BrowseTabs.kt`, `TrackContextMenu.kt`, `MobileSurfaceViewModel.kt`, `android/app/build.gradle.kts`, their tests |
| A | A4 | `BrowseScreen.kt`, `MainActivitySurface.kt`, `MainActivity.kt`, `LibrarySession.kt`, a new window-loader file, `AndroidPlaybackTestFixtures.kt`, their tests |
| B | B1 | `LibraryTrackRows.kt`, `BrowseTabs.kt`, `LibraryListAnchor.kt`, a new prefetch file, their tests |
| B | B2 | `ArtworkCache.kt`, `TrackCover.kt`, `RepriseApplication.kt`, `MainActivity.kt`, their tests |

`android/app/build.gradle.kts` appears twice, in different waves, on purpose. `MainActivity.kt` is at 753 lines; new wiring belongs in new files, not in it.

Wave C is the operator's: measure, tune the constants, write the numbers down. It is described at the end so the constants above are recognisable as provisional, but it is not implemented in this task.

---

## Wave 0 — a measurement someone else can repeat

### Intent

Produce a procedure that answers three questions with numbers: *how long does a cover take to arrive*, *how does the list scroll*, *how much memory do covers hold*. **You write it. You do not run it.**

### Entry points

**The script** — `scripts/android-scroll-baseline.sh`, following the shape of `scripts/performance-baseline.sh` (usage block, refuses an existing output dir, writes a manifest that names the commit). It should:

- Build the native library for the device — `ANDROID_TARGET=aarch64-linux-android ANDROID_ABI=arm64-v8a scripts/android-build.sh` — then `:app:assembleRelease` and install. The release type is signed with the debug key (`android/app/build.gradle.kts`), so it installs directly.
- Record the run's conditions into the report header: `wm size`, `dumpsys display | grep -E 'renderFrameRate|hasArrSupport'`, `dumpsys thermalservice`, battery level, and the three `settings get global *_animation_scale` values. A thermally throttled run must be recognisable after the fact rather than believed.
- Force-stop, launch, wait for the library screen, then a **warm-up pass**: one full scroll down and back. Cold-cache and warm-cache are two named scenarios, not one blurred one.
- `dumpsys gfxinfo org.reprise reset`, **then** N identical flings — `input swipe` with coordinates derived from `wm size` and a fixed short duration so it flings rather than drags, with fixed pauses between them — then `dumpsys gfxinfo org.reprise` and `dumpsys meminfo org.reprise -d`.
- Parse and print: total frames, janky frames and their percentage, the 50th/90th/95th/99th percentiles, missed vsyncs; TOTAL PSS, Native Heap and Graphics from meminfo; and the artwork counters below. Keep the raw dumps in the output directory.

Since you cannot run it, it must fail loudly rather than silently: check for `adb`, for exactly one connected device, for the package being installed, and for each `dumpsys` section actually being present before parsing it. A parser that silently prints zeros for a section that moved is worse than one that stops.

**The counters** — a small telemetry object beside `TrackArtwork` recording, per lane: requests made, cache hits, requests dropped by the gate, resolves completed, and the resolve and decode durations as a bounded histogram (count, median, p90, max). Update it on the worker thread where the work already happens. Expose it by overriding `Activity.dump()` in `MainActivity` so it can be read with `adb shell dumpsys activity top` — no debug build, no logging. The counters must be a **pull**, never a push, and reading them must not perturb what they measure.

Add `androidx.tracing` sections around the resolve and the decode as well, so the same run can later be opened in Perfetto without touching the code again. If that needs `androidx.tracing:tracing-ktx`, P0 owns the Gradle file this wave.

### Non-goals

No macrobenchmark module. No profileinstaller or baseline profiles (a real gain, but a different plan — and it would confound every before/after in this one). **No optimisation of any kind in this wave**: the numbers must describe today's code, so this wave must not change how anything behaves.

### Gate

The unit suite, unchanged in behaviour and risen in count by whatever tests you add for the telemetry object (histogram bounds, counter arithmetic, and that `dump()` produces parseable output):

```bash
export JAVA_HOME=/usr/lib/jvm/java-21-openjdk
export ANDROID_HOME="$HOME/.local/share/android-sdk"
export TMPDIR=/tmp
cd android && ./gradlew --max-workers=2 \
  -Pkotlin.compiler.execution.strategy=in-process :app:testDebugUnitTest
```

JDK 21 is mandatory — JDK 26 breaks Robolectric. `TMPDIR=/tmp` is mandatory — on NVMe the FFI tests go red on readdir ordering. Count the XML in `android/app/build/test-results/testDebugUnitTest/` and quote suites, tests, failures. Record the figures from a clean run **before** you change anything; the count must rise, never fall.

Also required, because the script is a deliverable you cannot execute:

```bash
bash -n scripts/android-scroll-baseline.sh
shellcheck scripts/android-scroll-baseline.sh   # if available
```

---

## Wave A — four independent strands

### A1 — the cover pipeline stops being a single queue and a single shelf

**Intent.** Covers for the rows on screen arrive first and stay cached long enough to survive a trip to the player and back.

**Entry points.** `TrackCover.kt` (the two executors at line 41–42, `loadVisual`, `seedVisual`, `decode`) and `ArtworkCache.kt`.

- Replace the single list executor with a small pool. The work is I/O-bound — binder, file, decode — so 2–4 threads sized from `Runtime.getRuntime().availableProcessors()` is the range; the exact number is a constant Wave C tunes. Keep the NOW_PLAYING lane separate: `TrackArtworkTest.nowPlayingArtworkUsesItsOwnLaneInsteadOfWaitingBehindListWork` pins that and must stay green.
- Serve the queue **newest-first**. During a fling most queued requests belong to rows that are already gone; the gate discards their results but they still occupy a worker in FIFO order. A stack-ordered work queue (`LinkedBlockingDeque` used as a LIFO by a `ThreadPoolExecutor`) puts the rows the finger just stopped over at the front. This is the single largest perceived win available. Note the tradeoff and do not paper over it: under a slow continuous scroll, LIFO can starve a request that is still visible. With a 2–4 thread pool and sub-100 ms resolves the starvation window is short, but if a test can express the bound, write it.
- Split the cache by size and bound it by **bytes**, not entries: a LIST shelf of 16 MB, a NOW_PLAYING shelf sized for three covers, and a small separate shelf for generated fallbacks so they are never evicted by resolved covers. Measure an entry by its bitmap's `allocationByteCount`. Sixteen megabytes is roughly 140 list covers, which makes scrolling back up free.
- Give `decode` a `BitmapFactory.Options`. ARGB_8888 stays (decision 3); what changes is decoding with explicit options rather than the no-options path that applies density scaling.
- Shrink the main thread's share of the instance lock. `seedVisual` is called during composition and takes the same monitor the workers hold; a read path that does not block behind a worker's write (a concurrent map for the lookup, or a lock held only around the LRU bookkeeping) removes a main-thread stall that no profile will ever name.

**Non-goals.** Do not change what a cover *looks like*. Do not give list rows per-track fallbacks (decision 7). Do not add any network path — nothing is downloaded, ever.

**Gate.** The unit suite, with the whole of `TrackArtworkTest`, `ArtworkCacheTest` and `ArtworkCompositionTest` green and the test count risen. New tests must pin at least: a byte budget that evicts by bytes and not by count; that a NOW_PLAYING entry cannot evict the whole LIST shelf; that the newest request is served before an older queued one; and that the gate still discards a superseded request before doing the work (`ArtworkRequestGate` identity semantics, unchanged).

### A2 — the tag read happens once, not once per launch

**Intent.** A cover whose thumbnail is already on disk must not require opening the audio file through the DocumentsProvider.

**Entry points.** `crates/reprise-core/src/cover.rs` — the resolution index at `cover.rs:349–435`, today reachable only through `thumbnail_for_track` and hard-wired to the XDG `cache_dir()` — and `MusicLibrary::track_artwork` (`crates/reprise-android-ffi/src/lib.rs:237`), which calls `resolve_source_with_source` + `thumbnail_with_source` directly and so pays the tag read every time.

Per decision 2, the shape is settled: make the stamp an **argument** to `resolution_stamp` rather than something it reads via `std::fs::metadata`, and give Android a source-aware, cache-root-aware entry point equivalent to `thumbnail_for_track`. The desktop keeps supplying the stamp from the filesystem; Android supplies `(tracks.file_mtime, tracks.file_size)` from the row it is already reading. Keep the index a file in the cache root as it is today — no schema change.

Keep the two answers of `track_artwork` strictly apart — `Ok(None)` means "this track has no artwork", `Err` means "the library could not answer". The doc comment at `lib.rs:222–235` explains what folding them together cost.

**Non-goals.** No schema migration. No change to the thumbnail sizes (168 / 1092 in `ThumbnailSize`, `cover.rs:165`); 168 px is exactly 56 dp at this device's density and is correct. No download path.

**Gate.**

```bash
cargo test -p reprise-core cover
cargo test -p reprise-android-ffi
cargo fmt --check
cargo clippy -p reprise-core -p reprise-android-ffi -- -D warnings
scripts/check-architecture.sh
ANDROID_TARGET=aarch64-linux-android ANDROID_ABI=arm64-v8a scripts/android-build.sh
```

Quote the test counts. A new test must show that a second `track_artwork` for the same track with a warm cache does **not** open the file — a counting `LibrarySource` proves it, and `crates/reprise-android-ffi/src/artwork_tests.rs` already builds such fixtures. A second test must show the desktop path still resolves correctly with the stamp now passed in.

### A3 — rows recompose for their own reasons only

**Intent.** A play-position tick stops touching the list, and the row composables become skippable.

**Entry points.** `LibraryTrackRows.kt`, `LibraryFramePolicy.kt:44`, `BrowseTabs.kt`, `TrackContextMenu.kt:78`, `MobileSurfaceViewModel.kt`, `android/app/build.gradle.kts`.

- **Prove it before and after.** Wire the Compose compiler reports behind a Gradle property so ordinary builds are unaffected:
  ```kotlin
  if (project.findProperty("reprise.composeReports") == "true") {
      composeCompiler {
          reportsDestination = layout.buildDirectory.dir("compose_reports")
          metricsDestination = layout.buildDirectory.dir("compose_metrics")
      }
  }
  ```
  `app_release-composables.txt` names every composable `restartable`/`skippable` or not; `app_release-classes.txt` names every class stable or not, with the offending field. That file is the evidence, and it needs no device.
- Stop handing rows the whole `PlaybackUiState`. `playbackPresentation` reads only `currentTrackId` and `isPlaying` (`LibraryFramePolicy.kt:44–52`) — never `positionMs` — so the derived value is identical across a tick. Derive the minimum the list needs with `derivedStateOf` so item lambdas read a value that only changes when the *track* changes. The shape of `TrackPlaybackPresentation` (`LibraryFramePolicy.kt:38`) is already right.
- Make the row's dependency on the ViewModel stable. Either annotate `MobileSurfaceViewModel` `@Stable` — defensible, since every property the composition reads is `mutableStateOf`/`mutableStateMapOf`, but note that `scrollPosition()` reads a plain map and is only read at composition start — or, cleaner, give the row a narrow `@Stable` interface exposing just `ratingOf`. **Do not hoist the rating out of the row**: reading it inside the row is what makes a heart tap recompose one row instead of the list, and `LibraryRatingVisibilityTest` plus the reasoning at `MobileSurfaceViewModel.kt:243–262` say why one place owns it.
- Replace the `composed { }` in `trackContextMenuAnchor` with a modifier that has equality — a `@Composable` factory that reads the locals and returns a plain chain is the small step; a `ModifierNodeElement` is the thorough one.
- Add `contentType` where the buckets are genuinely mixed: `BrowseTabs.kt:339` and `:408`. Add it to the homogeneous lists too, for the sentinel. **Keys stay exactly as they are** — `libraryRowKey`/`queueRowKey` (`LibraryTrackRows.kt:141–156`) encode a hard-won distinction and `LibraryListAnchorTest` plus #449 depend on the anchor behaviour that follows from them.

**Non-goals.** No `@Immutable` on `LibraryTrack` and friends — already inferred stable. No visual change. Not the settings screens.

**Gate.**

```bash
cd android && ./gradlew :app:assembleRelease -Preprise.composeReports=true
grep -E "LibraryTrackRow|TrackListItem|TrackCover" \
  android/app/build/compose_reports/app_release-composables.txt
```

Expected: `restartable skippable` on `LibraryTrackRow` and `TrackListItem`. Quote the lines before and after. Plus a Robolectric test — the harness exists, see `ArtworkCompositionTest` — that counts compositions of a row while the playback state is advanced by 500 ms with the track unchanged, and asserts the count does not rise. Plus the unit suite green with a higher test count.

### A4 — paging leaves the main thread and stops being visible

**Intent.** No SQLite query on the main thread, and no "Loading…" row where a listener can see it.

**Entry points.** `BrowseScreen.kt:333–405` (the six `loadMore*` functions), the lambdas that feed them from `MainActivitySurface.kt`/`MainActivity.kt`, and a new loader file.

Build the loader in the image of `TrackLoader.kt` — which exists for this exact lock and whose doc comment is the specification: request off the main thread, answer on it, a superseded request abandoned, teardown discards rather than drains. `PlaybackQueryRunner` (`ActivityPlaybackControls.kt:20`) is the second precedent. Windows differ from `TrackLoader` in one respect: a window that arrives late is still wanted if it is still the *next* window, so the guard is the requested offset, which `titlesRequestedOffset` and its siblings already are.

Keep `loadMore: (LibraryWindowRange) -> Unit` as the type the list sees, so `LibraryTrackRows.kt` needs no change this wave — A3 owns that file.

While you are here: `LibraryWindow.append` (`LibraryScreenState.kt:26`) copies the whole row list per page, so a 10 000-track library copies ~50 lists on the way down. Not the headline, but a cheap fix if the shape allows one.

**Non-goals.** Do not change the window size (decision 6). Do not touch the anchor restore; #449 landed on it four commits ago.

**Gate.** The unit suite green with a higher test count, including a new test proving a window request does not run its query on the calling thread and that a superseded request is not delivered. `BrowseSurfaceTest` and `MainActivity*Test` must stay green — they construct the surface whose lambdas change type.

### Review after Wave A

An adversarial read of the whole diff before Wave B starts, with the Compose report in hand.

---

## Wave B — look ahead

### B1 — covers and rows arrive before the finger does

**Intent.** The listener never sees a cover appear and never sees the loading sentinel.

**Entry points.** `LibraryTrackRows.kt`, `BrowseTabs.kt`, `LibraryListAnchor.kt`, a new prefetch file.

- Drive `TrackArtwork.prefetch` from the list. It is already written (`TrackCover.kt:114`) and has exactly one caller, for the playback queue (`MobileSurfaceViewModel.kt:187`). Read `LazyListState.layoutInfo` and ask for the covers of the rows just beyond the viewport, in scroll direction. Prefetches must be **lower priority than visible rows** — with A1's stack-ordered queue that means the bottom of the deque, not the top. Watch the allocation cost: reading `layoutInfo` in a `snapshotFlow` runs per frame, so debounce or derive rather than recompute.
- Ask for the next window before the sentinel is reached — when the last visible index comes within a screenful or two of `rows.size` — so `loadMore` (async since A4) has landed by the time the listener gets there. The sentinel stays as the backstop it is.
- While in `LibraryListAnchor.kt`: the per-frame `LibraryScrollPosition` allocation in `ObserveLibraryListAnchor`. The anchor only needs to be *durable*, not live — sampling it, or comparing before allocating, removes an allocation per frame per list without changing what is restored. `LibraryListAnchorTest` and #449 define what must not change.
- Build the 120 Hz request behind the existing pattern: a testable function that decides the category (see `requestedVisualizerFrameRateCategory`, `NowPlayingScene.kt:74–78`) plus `Modifier.preferredFrameRate(...)` applied while the list is in motion. Per decision 5, **leave it switched off by default** — a constant or a policy function that currently returns `null` — so Wave C can turn it on once the measurement shows frames under 8.3 ms. Say plainly in the commit message that it is built but dormant.

**Non-goals.** Prefetching whole windows speculatively (paging is a database read, not a picture). Prefetching NOW_PLAYING covers for list rows.

**Gate.** Unit suite green, test count up, with tests proving prefetch requests are issued for rows outside the viewport, that they never displace a visible row's request, and that the frame-rate policy returns nothing while dormant.

### B2 — the memory has a budget and gives it back

**Intent.** Cover memory is bounded by something the device chooses, and the play view stops allocating megabytes on the main thread.

**Entry points.** `ArtworkCache.kt`, `TrackCover.kt` (`seedVisual`), `RepriseApplication.kt`, `MainActivity.kt`.

- Move the NOW_PLAYING fallback off the main thread. `seedVisual` (`TrackCover.kt:143`) building a 1092² ARGB_8888 bitmap inside composition is 4.77 MB and a gradient fill, three times over when `NowPlayingScene` seeds current, previous and next (`NowPlayingScene.kt:96,103,111`). Seeding must return instantly — the cached visual if there is one, otherwise `null`, with the generated cover arriving from the worker like everything else. **`ArtworkCompositionTest.cached_cover_is_non_null_in_the_first_composition` and `TrackArtworkTest.aCachedCoverIsDeliveredSynchronouslyWithoutAnEmptyFrame` are the boundary**: a *cached* cover must still be there in the first frame, and a track without artwork must still end up with a generated image and never a null or a block of accent colour (`aTrackWithoutArtworkReceivesAGeneratedCoverInsteadOfTealOrNull`). If dropping the synchronous seed would show an empty frame where one is not acceptable, keep the seed for LIST — where it is one shared cached bitmap — and drop it only for NOW_PLAYING.
- Honour `onTrimMemory` / `ComponentCallbacks2`. A cover cache that never shrinks is a cache the system kills the process over. `RepriseApplication` is the natural place; `SharedArtworkCache` is a process-wide singleton (`ArtworkCache.kt:105`) and outlives every activity, which is a feature as long as something can empty it.

**Non-goals.** Do not make the cache persistent on disk — the thumbnails already are, that is what A2 is about.

**Gate.** Unit suite green with a higher test count, including a test that the cache releases entries on a trim callback and that the play view's first composition allocates no full-size bitmap.

### Review after Wave B

Same shape as after Wave A.

---

## Wave C — the operator's wave, not yours

Not implemented in this task. Recorded so the constants above are recognisable as provisional: re-run the baseline for every scenario, twice each, compare, then tune worker-pool size, the two cache budgets, prefetch distance, the 120 Hz switch and only then the window size. Write a "Measured" section into this document with the device, the date and the thermal state.

---

## Behaviour that must not change

- **The recycle token.** `ArtworkRequestGate` admits only the newest request a slot made, by identity, and a result for a scrolled-away row is discarded (`ArtworkRequestGate.kt`, `TrackCover.kt:66,88,96`). Concurrency changes make this *more* important, not less. Pinned by `TrackArtworkTest` throughout.
- **Nothing is downloaded.** A track without a local cover keeps the deterministic generated image. Pinned by `TrackArtworkTest.aTrackWithoutArtworkReceivesAGeneratedCoverInsteadOfTealOrNull` and `ArtworkCacheTest.generated_cover_is_a_dark_gradient_and_never_the_old_teal_accent`.
- **Teardown discards rather than drains.** `TrackArtwork.shutdown` must keep stopping *both* lanes and must keep being safe against a handle closed underneath a running read — the reasoning is in the doc comment at `TrackCover.kt:170–195` and is pinned by `shutdownStopsTheFullSizeLaneAndNotOnlyTheListLane`, `aReadThatFindsTheLibraryClosedIsAnsweredRatherThanEndingTheProcess` and `aCallMadeAfterTheHandleIsClosedIsRefusedRatherThanPassedToNativeCode`. A thread pool must still be discardable in one call.
- **List keys.** The library keys by uri and the queue keys by slot, because the queue allows duplicates and a uri-only key throws (`LibraryTrackRows.kt:126–156`). `contentType` is added *beside* the keys, never instead.
- **The restored viewport.** `LibraryListAnchorTest`, and #449 (`2524f052dc`) which deferred the restore until the list is allocated.
- **The rating shown for a row** comes from one place (`MobileSurfaceViewModel.ratingOf`), never from a per-surface copy. `LibraryRatingVisibilityTest`.

---

## Entry points at a glance

Not an exhaustive list — the packages above define ownership, and each may change adjacent code inside its own area.

- `android/app/src/main/java/de/reprise/spike/TrackCover.kt`
- `android/app/src/main/java/de/reprise/spike/ArtworkCache.kt`
- `android/app/src/main/java/de/reprise/spike/LibraryTrackRows.kt`
- `android/app/src/main/java/de/reprise/spike/BrowseScreen.kt`
- `crates/reprise-core/src/cover.rs`
