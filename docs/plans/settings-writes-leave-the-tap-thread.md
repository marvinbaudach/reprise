---
slug: settings-writes-leave-the-tap-thread
worktree: /home/marvin/Projects/reprise-settings-writes-leave-the-tap-thread
branch: feature/settings-writes-leave-the-tap-thread
phase: planned
codex_session:
created: 2026-08-27
---
# The settings writes leave the tap thread

## Goal

A tab switch must never put the main thread behind a folder scan. Today it does,
and the phone ANRs.

Every UI-triggered `MusicLibrary` **write** moves onto one named background lane
— the pattern this app already built for exactly this failure in
`RatingWriter`. Nothing about *when the control moves* changes: the answer still
arrives on the main thread, and the control still waits for the database to
agree. It just stops blocking the tap.

## The evidence

Four ANR traces on the connected Pixel 10 Pro XL, `/data/anr/`, all dated
2026-08-27: `00-38-30-386`, `11-12-34-262`, `14-25-21-601`, `14-25-40-372`. Two
come from a different APK install hash than the other two, so this is not one
bad build. All four carry the same subject and the same main-thread stack:

```
Subject: Input dispatching timed out (… de.reprise.spike.MainActivity
         is not responding. Waited 5000ms for MotionEvent).

"main" prio=5 tid=1 Native
  <std::sys::sync::mutex::futex::Mutex>::lock_contended
  <reprise_android_ffi::library_types::MusicLibrary>::writer
  <reprise_android_ffi::…>::set_library_destination
  → uniffi … MusicLibrary.setLibraryDestination
  → android.os.Handler.handleCallback / Looper.loop
```

and, in the same dump, the thread holding the lock:

```
"Thread-2" prio=5 tid=16 Native
  ioctl → IPCThreadState::transact → ContentResolver.query
  → de.reprise.spike.AndroidSafSource.listChildren (AndroidSafSource.kt:62)
  → uniffi … uniffi_reprise_android_ffi_fn_method_musiclibrary_scan
  → MusicLibrary.scan
```

In the `00-38` dump a third thread is queued behind the same mutex:
`"reprise-analysis"` in `MusicLibrary::import_track_analysis`. The scan does not
only freeze the UI; it starves the background importer too.

**This is the control arm, already collected.** It is the pre-fix build, on the
real library, with the real gesture. Task 4 does not rebuild it.

## The chain

1. `MusicLibrary::scan` (`crates/reprise-android-ffi/src/lib.rs:169`) takes
   `self.writer()` **once, for the whole scan**. Inside, `scan_folder_inner`
   opens one SQLite transaction (`crates/reprise-core/src/library/scanner.rs:300`)
   held across the entire SAF walk — by design, documented at `scanner.rs:171-260`:
   the scan *is* the reconcile, so it must be one transaction. Every folder step
   is a binder round-trip through `AndroidSafSource.listChildren`. On a real
   library that is minutes.
2. A tab switch reaches `MobileSurfaceViewModel.selectTab`
   (`MobileSurfaceViewModel.kt:219`), which calls `rememberSelectedTab(tab)`
   **synchronously** (`:222`).
3. That lambda is `MainActivity.rememberBrowseTab` (`MainActivity.kt:407-411`),
   which calls `library.setLibraryDestination(destination)` (`:409`) — a blocking
   JNI call wanting the same writer mutex.
4. Main thread parks. Five seconds later: ANR.

Both ways into the tab funnel through that one seam: the nav bar and rail
(`LibraryFrame.kt:187`, `:236`) and the pager swipe (`BrowseScreen.kt:171-176`,
`snapshotFlow { pagerState.settledPage }` collected on the Compose main
dispatcher → `selectDestination` → `selectTab`). The `Handler.handleCallback`
frames in the trace are the swipe path.

Introduced 2026-08-08 in #371, the commit that added the bottom bar and the
swipe. It only bites while a scan runs, which is why "right after start" is the
reproduction: `MainActivity.onResume` kicks off `autoScan`
(`MainActivity.kt:449-457`).

## Why this shape

**The precedent is in the tree with the diagnosis written on it.**
`RatingWriter.kt:22-38`:

> A rating is a SQLite `UPDATE` behind the same handle a SAF scan holds for the
> whole of its folder walk, so writing it where the tap happens puts the main
> thread behind a transaction of unbounded length.

That is this bug, word for word. Ratings were moved onto a named single thread;
the preference writes never were. This plan finishes the job rather than
inventing anything.

**Ruled out: fixing the Rust side.** `writer` and `reader` are two separate
`rusqlite::Connection`s on the same file (`lib.rs:87`, `:92`), which is why reads
already survive a scan — `crates/reprise-android-ffi/src/read_during_scan_tests.rs:298-330`
proves it. Writes cannot get the same treatment: SQLite allows one writer at a
time, so a third connection would sit on `busy_timeout` (5 s,
`crates/reprise-core/src/db.rs:32`) and then fail `SQLITE_BUSY` for the rest of
the scan. Releasing the Rust mutex mid-scan means splitting the scan's single
transaction, which is the one thing `scanner.rs:171-260` says must not happen.
The scan holding the writer is a real cost — it is what starves
`reprise-analysis` — but it is separate, architectural work.

**Ruled out: fixing only `setLibraryDestination`.** It is not special. Every
short setter on `MusicLibrary` contends for the same mutex, and three more are
still called straight from a Compose handler. Same bug, waiting for the same
scan.

## What is already there — verified, do not rebuild

- **The house pattern**, six times over: a named single-thread `ExecutorService`
  wrapped in a small class with `runCatching` and a hop back to main —
  `reprise-ratings` (`RatingWriter.kt:84`), `reprise-track` (`TrackLoader.kt:153`),
  `reprise-analysis` (`TrackAnalysisLoader.kt:127`), `reprise-artwork-list`/`-full`
  (`TrackCover.kt:307-308`), `reprise-artist-portraits`
  (`ArtistPortraitPrefetch.kt:63-65`), `reprise-playback-queries`
  (`ActivityPlaybackControls.kt:21-23`). Nothing in `de/reprise/spike/` uses
  `Dispatchers.IO` for a `library` call. Do not introduce one.
- **The reader/writer split.** Readers tear down with `shutdownNow()`
  (`TrackLoader.kt:147`, `TrackCover.kt:222`); writers drain with a bounded
  `awaitTermination` (`RatingWriter.kt:69-79`, `TrackAnalysisLoader.kt:109-119`).
- **The teardown order** is already correct and commented (`MainActivity.kt:478-508`):
  writers drain before the library handle closes, readers are dropped.
- **Reads during a scan are safe.** The reader connection is untouched by the
  scan. This plan does not move a single read.
- **Scans cannot overlap each other**: `LibrarySession` serialises
  `chooseTree`/`rescan`/`autoScan` on `scanMonitor` (`LibrarySession.kt:83`). The
  gap is only between a scan and everything *outside* `LibrarySession` — exactly
  the set this plan moves.
- **`RatingWriter` already takes its executor as a parameter**
  (`RatingWriter.kt:42`), and no test anywhere asserts a thread name. Sharing the
  lane costs no test rewrite beyond the drain call itself.

## The four writes, and what each one promises

Verified at the call site, not assumed:

| write | FFI site | Compose site | promise today |
|---|---|---|---|
| `setLibraryDestination` | `MainActivity.kt:409` | tab tap / pager swipe | none — `runCatching{}.onFailure{Log.e}`; the tab already moved |
| `setTheme` | `ThemeSelection.kt:49` | `MainActivity.kt:294-300` | `themeSelection` is set **only `.onSuccess`** |
| `setVisualizer` | `VisualizerPreference.kt:26` | `NowPlayingSheet.kt:161-173` | `visualizerVisible` is set **only `.onSuccess`** |
| `setOnlineSourcesEnabled` | `MainActivity.kt:386` | `MainActivity.kt:283-292` | `onlineSourcesEnabled` is set **only `.onSuccess`** |

Three of the four already wait for the database. **That contract is kept.** The
tap returns immediately and the control moves when the answer arrives on the main
thread — precisely `RatingWriter.setFavourite`. Optimistic UI was considered and
rejected: a control that flips on tap and silently reverts two minutes later is
its own defect, and the sluggishness it would paper over is the scan-holds-the-
writer problem, which has its own plan.

## Decisions taken in the grill

1. **One lane, not one thread per concern.** All these writes contend for the
   same Rust mutex; one queue is the honest model. Two lanes would also mean two
   sequential drains in `onDestroy` — up to 4 s on the main thread during a scan,
   and a rotation goes through that path.
2. **The lane owns the thread and the drain.** `RatingWriter` loses its own
   executor and its `shutdown()` rather than keeping a method that would kill a
   shared lane if anyone called it.
3. **The contract holds, asynchronously** (above).
4. **The seam sits at the Compose call site**, because the whole existing
   synchronous call can move onto the worker unchanged. `ThemeController`,
   `ThemeSettingsPort`, `ThemeSelectionTest` and `MainActivitySurface`'s
   signatures are therefore **not** touched. Only `VisualizerPreference` changes
   shape, because `NowPlayingSheet` reaches the library solely through
   `LocalVisualizerPreference` and cannot see the lane.
5. **A debug assertion, not a grep gate.** The chosen design still calls
   `library.setX` inside `MainActivity` — correctly, inside a `submit` lambda — so
   a text scan cannot tell inside from outside without either being holey or
   dictating the design. A runtime check at the write itself is exact.
6. **The drain waits only when something is waiting for an answer.** Before this
   plan `onDestroy` waited only when a rating happened to be queued — rare.
   Afterwards something is queued during *every* scan, the worker is parked in
   the mutex for minutes, and a flat 2000 ms `awaitTermination` would run out in
   full on every rotation.

   The exact rule, and it is deliberately coarser than "drop the unanswered
   ones": **if no answered work is pending, tear down at once and lose whatever
   is queued; otherwise drain as before, up to 2000 ms.** One counter, one
   executor — an unanswered item sitting ahead of an answered one in the same
   FIFO is drained along with it. That costs nothing (it is one settings write)
   and buys the common case, which is a rotation during a scan with only a tab
   switch queued. Two queues to make the rule exact would be more machinery than
   the difference is worth.

7. **The timeout path does not call `shutdownNow()`** (ruled 2026-08-28, after
   review finding M1). `shutdownNow()` discarded a queued answered task, so its
   `report` never fired and the pending counter leaked. Dropping the call does
   not lengthen teardown in any way that matters: the drain only times out
   because a write is parked in the library writer inside a JNI call, and
   `Thread.interrupt()` does not unblock that — so `shutdownNow()` never bounded
   the *running* task's lifetime, only the queued tail, which is short setter
   writes finishing in milliseconds once the scan releases the writer. Decision
   6 is untouched: the caller still waits at most 2000 ms.

## Task 1 — `LibraryWrites`, red first

New: `android/app/src/main/java/de/reprise/spike/LibraryWrites.kt` and
`android/app/src/test/java/de/reprise/spike/LibraryWritesTest.kt`.

Write the tests first. Plain JUnit — the class touches neither Compose nor
Android framework types beyond `Looper` in the assertion helper, which the tests
must not need.

1. `theWriteLeavesTheCallersThread` — the submitted work records
   `Thread.currentThread()`; assert it is not the caller's and that its name is
   `reprise-library-writes`.
2. `theCallerReturnsBeforeABlockedWriteFinishes` — submit work that blocks on a
   latch; assert `submit` has returned while the latch is still closed. **This is
   the ANR, in a test.** It cannot be green before this task exists.
3. `writesReachTheDatabaseInTheOrderTheyWereTapped` — three submits, assert the
   recorded order. One thread rather than a pool is load-bearing: the last tab
   tapped must be the one stored.
4. `anAnsweredWriteIsAnsweredExactlyOnceThroughTheHop` — the report arrives via
   `onMainThread`, once, carrying the work's return value.
5. `aFailingWriteIsReportedAndTheLaneKeepsRunning` — a throwing write is caught,
   handed to the report/failure hook, and the next write still runs.
6. `workSubmittedAfterShutdownIsReportedNotThrown` — `RejectedExecutionException`
   becomes a reported failure, mirroring `RATING_WRITER_STOPPED`.
7. `shutdownDrainsAnsweredWork` — an answered write queued behind a slow one is
   still executed, and `shutdown` returns `true`.
8. `shutdownDoesNotWaitWhenNothingIsWaitingForAnAnswer` — with **only**
   unanswered work queued behind a blocked write, `shutdown` returns promptly
   rather than spending the full timeout. Assert against the timeout, not
   against a wall-clock guess. The name has to say "nothing answered is
   pending", not "unanswered work is dropped": one answered item in the queue
   drains the unanswered ones too, by Decision 6.

Then the class:

```kotlin
internal class LibraryWrites(
    private val onMainThread: (() -> Unit) -> Unit,
    private val worker: ExecutorService = singleLibraryWriteThread(),
) {
    /** Persistence nobody is waiting for. Dropped at teardown, never awaited. */
    fun submitUnanswered(work: () -> Unit, onFailure: (Throwable) -> Unit)

    /** The control moves when this answers — exactly once, on the main thread. */
    fun <T> submitAnswered(work: () -> T, report: (Result<T>) -> Unit)

    fun shutdown(): Boolean
}

private fun singleLibraryWriteThread(): ExecutorService =
    Executors.newSingleThreadExecutor { Thread(it, "reprise-library-writes") }
```

`shutdown` keeps a count of answered work still pending — incremented at submit,
decremented once the report has been handed over. At zero it calls
`shutdownNow()` and returns `true` immediately; otherwise `shutdown()` plus
`awaitTermination(DRAIN_TIMEOUT_MS)`, and **no `shutdownNow()` on timeout** —
see Decision 7. **One counter, one executor.** This is Decision 6 exactly as
worded there; do not build a second queue to make the rule finer.

Carry `RatingWriter`'s doc-comment discipline: say *why* the lane exists (cite
the scan holding the writer for its whole folder walk), and state the
answered/unanswered drain rule and its reason where a reader will find it.

Also in this task, the guard Task 3 installs. It goes in its **own file** —
`LibraryWrites` stays free of Android framework types so `LibraryWritesTest` can
be plain JUnit:

```kotlin
internal fun requireOffMainThread(what: String) {
    if (!BuildConfig.DEBUG) return
    val main = Looper.getMainLooper() ?: return   // a JVM test has no looper
    check(Looper.myLooper() !== main) {
        "$what writes on the main thread; queue it on LibraryWrites"
    }
}
```

It must be harmless in a plain JVM test and under Robolectric — hence the null
check on `getMainLooper()`.

**Where it goes, and where it deliberately does not.** The guard exists to catch
a *future* caller that forgets the lane, so it only earns its place on a path
such a caller could reach:

- `AndroidThemeSettingsPort.setTheme` and `AndroidVisualizerPreference.setVisualizer`
  — yes. Both are ports; anything holding one can call them directly, and that
  call would be synchronous on whatever thread it happens on.
- `rememberBrowseTab` and the online-sources lambda at `MainActivity.kt:386` —
  **no.** Both are private to `MainActivity` and reachable only through the
  `submit` written in Task 3, so an assertion there runs on the worker by
  construction and can only ever pass. Two assertions that cannot fail are worse
  than none: they read as coverage.

That leaves `setLibraryDestination` — the measured ANR — without a runtime
guard. Giving it a port purely to hold an assertion is scope this bugfix should
not take; what protects it instead is Task 1's test 2 and the fact that its one
call site is three lines long.

## Task 2 — `RatingWriter` becomes a user of the lane

- Constructor takes `LibraryWrites` instead of an `ExecutorService`.
- `setFavourite` delegates to `submitAnswered`; its per-tap `report` contract and
  its `RATING_WRITER_STOPPED` answer are unchanged.
- `shutdown()` is removed. Its doc comment's reasoning (a queued write must not
  reach a closed handle) moves to `LibraryWrites.shutdown`.
- `RatingWriterTest`: the four `writer.shutdown()` calls become the lane's, and
  the test constructs a lane. The assertions themselves do not change — including
  `assertTrue("teardown must drain what was queued", …)`, which now exercises the
  answered path.

## Task 3 — route the four writes

- **Construct the lane in `MainActivity` before `ratings`** (which is at
  `MainActivity.kt:106`), with `onMainThread = { work -> runOnUiThread { work() } }`.
- **`onDestroy` (`:489`)**: `libraryWrites.shutdown()` replaces `ratings.shutdown()`,
  keeping the warning line. Update the comment above it — it currently explains
  the rating drain; it now explains when the drain waits and when it does not
  (Decision 6). The rest of the teardown order is unchanged.
- **`rememberBrowseTab` (`:407-411`)** → `submitUnanswered`, keeping the existing
  `Log.e` as `onFailure`. No assertion here — see Task 1 on why one would be
  unreachable.
- **`setOnlineSourcesEnabled`**: the Compose site (`:283-292`) wraps its existing
  `surface.setOnlineSourcesEnabled(enabled)` call in `submitAnswered`, using
  `getOrThrow()` inside the work so the report is a flat `Result<Unit>`;
  `onlineSourcesEnabled = enabled` and the `Log.e` move into the report.
  No assertion here either. The `onSuccess` in the lambda at `:385-393` — which
  starts or cancels the portrait backfill — touches Compose state, so it moves
  into the report and stays on main.
- **`selectTheme`**: the Compose site (`:294-300`) wraps
  `surface.selectTheme(themeSelection, palette)` in `submitAnswered`;
  `themeSelection = selection` and the `Log.e` move into the report.
  `requireOffMainThread` goes into `AndroidThemeSettingsPort.setTheme`
  (`ThemeSelection.kt:49`) — `RecordingThemeSettingsPort` in the tests is a
  different implementation and is unaffected.
- **`setVisualizer`**: `VisualizerPreference.setVisualizer` gains a
  `report: (Result<Unit>) -> Unit` parameter.
  `AndroidVisualizerPreference` (`MainActivity.kt:132`, constructed as
  `AndroidVisualizerPreference { library }`) also takes the lane and submits;
  `DisconnectedVisualizerPreference` answers `Result.success(Unit)` synchronously,
  which is what it effectively does today. `NowPlayingSheet.kt:161-173` moves its
  `.onSuccess { visualizerVisible.value = showSpectrum }` into the report.
  `RecordingVisualizerPreference` in `NowPlayingGesturesTest.kt:353-366` gains the
  parameter and answers synchronously, so those five tests stay synchronous.
  `requireOffMainThread` goes into `AndroidVisualizerPreference.setVisualizer`,
  around the FFI call it makes on the worker.

`MainActivitySurface.kt` is deliberately **not** touched: `rememberBrowseTab`,
`selectTheme` and `setOnlineSourcesEnabled` keep their signatures because the
whole synchronous call moves onto the worker. `MainActivityConfigurationTest`'s
fake surface therefore needs no change either.

## Task 4 — gates and evidence

**Gates.** Derive the list from `scripts/check-merge-readiness.sh` itself rather
than from this plan; at minimum this branch needs the stage that has turned `dev`
red on an Android PR before:

- `scripts/check-android-suite.sh` — the JVM suite, floor 334. New tests raise
  the count; the floor is a floor. Needs JDK 21 and the generated UniFFI
  bindings; do **not** set `LD_LIBRARY_PATH` by hand, the script has done it
  since #645 and overriding it voids the evidence.
- `scripts/android-build.sh` first in a fresh worktree — `uniffi.reprise_android_ffi`
  is generated and gitignored, and `android/local.properties` must exist, or
  Kotlin compilation fails with unresolved bindings before any gate runs.
- `npm --prefix android run lint` and `npm --prefix android run test:lint`
  (`check-project-quality.sh --android`).
- `scripts/check-architecture.sh` — it invokes `check-android-theme.sh` at
  `:472`, which is the raw-Compose-colour text scan.

**Evidence on the device.** The control arm exists (the four ANR traces). Only
the fix arm is missing, and the app is currently not installed
(`pm path io.github.marvinbaudach.reprise` is empty).

1. Install the fixed build.
2. Trigger a scan and **prove positively that it is running** at the moment of
   the gesture — a thread dump showing a thread inside `listChildren`, or
   timestamped scan-progress lines in `logcat`. Without this step, step 4 only
   proves no scan was running.
3. While it runs, exercise all three changed contracts in the same scan window:
   - switch tabs by **tapping the nav bar and by swiping the pager** — both
     reach `selectTab`, and only the swipe is in the captured traces;
   - **change the theme** in settings;
   - **tap the cover** in Now Playing to flip the visualizer.

   The last two are where a control now moves later than the tap, and this is
   the only place that decision meets a human. Record what it actually looks
   like: the expected behaviour is that the switch lands once the scan releases
   the writer, not that it feels instant.
4. `adb shell ls /data/anr` gains no new `MainActivity is not responding` entry.

**Struck 2026-08-28 — steps 5 and 6 ("the last tab is remembered" after a
restart, and its negative case).** Measured on both arms: the tab switch lands
in the UI every time but survives no `am force-stop` + relaunch, on this branch
*and* on `origin/dev` 3000da8cd2. The premise is therefore not a property of
this diff and step 6's negative case is moot while a completed switch is
forgotten too. Whether the browse tab is meant to survive process death at all —
and what `setLibraryDestination` actually persists — is a separate product
question, tracked outside this plan.

Read the nav item *container* for the tab, not its `TextView`: the container
carries `selected="true"`, the TextView always says `false`.

## Not in this plan

- **The scan holding the writer mutex for its whole duration.** Real, measured
  (`reprise-analysis` blocked in the `00-38` trace), and architectural: it needs
  the single scan transaction (`scanner.rs:300`) split, or a write queue in Rust.
  Own plan. It is also the real cure for controls that feel sluggish during a
  scan.
- **The three playback-settings writes** — `setEqualizerEnabled`
  (`MainActivity.kt:694-698`), `replaceEqualizerCurve` (`:700-708`),
  `setGaplessEnabled` (`:710-714`). Each is write → `reloadPlaybackSettings()` →
  `return loadPlaybackSettings()`. Moving them means the settings screen stops
  being a synchronous `() -> PlaybackSettingsUiState` and becomes a state that
  arrives — a screen rewrite, not a dispatch change. They carry the same ANR
  risk and should follow soon.
- **The main-thread *reads*** in `onCreate` (`restoreStoredDestination`,
  `restoreTheme`, `LibrarySession.restore`). They cannot hit the scan mutex, so
  they are not this bug, but they are blocking IO on the main thread.
- **`docs/ux-rules.md`.** The rulebook has no `[android]` scope — 136 `[core]`,
  291 `[gtk]`, 21 `[web]`, 18 `[manual]`, zero `[android]` — and inventing one
  would make the rule invisible to the traceability gate. GP-2 ("no blocking I/O
  on the main thread") is `[gtk]`-scoped and names glib primitives. Opening an
  Android scope is a rulebook decision, not a bugfix.

## Known consequence to state in the commit

While a scan runs, a tab switch is persisted only after the scan finishes, and
`onDestroy` deliberately drops it rather than waiting. Kill the app mid-scan and
the last tab switch is lost. That is the trade: an unbounded main-thread block
becomes a bounded loss of one preference — the same terms `RatingWriter` already
accepted for a heart tap, except that a heart is still waited for, because
something is waiting for its answer.

## Parallelität

**Attempted, no cut. One strand.**

The change is one new class plus its tests, one migrated collaborator, and four
call sites that cannot compile until the new class exists. A strand owning only
the call sites would not build in its own worktree, let alone go green — the
failure mode the Flathub wave paid for. Splitting the call sites among strands is
worse: three of the four live in `MainActivity.kt`, so the file groups are not
disjoint. `RatingWriter` cannot be split off either — it is the file that gives
up the executor the new class must own, so its change and Task 1 are two halves
of one edit.

The debug assertion is not a separate file group either: it is asserted from
inside the four call sites Task 3 rewrites.

Post-merge cross-checks: none — nothing here is split across branches.
