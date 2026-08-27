# Handover — the mark follows the page

Written 2026-08-27. Covers the Android browse-pager work: the navigation mark
and the count line no longer wait for the swipe to settle, and the tab next
door is no longer drawn empty.

**Superseded on 2026-08-27 by the session below — read §0 first.**

The original work happened directly in the shared checkout `~/Projects/reprise`
on `fix/contrast-button-fixes`, not in a worktree. That is no longer where it
lives.

---

## 0 — Where this actually is now (2026-08-27, second session)

| | |
|---|---|
| worktree | `~/Projects/reprise-browse-prefetch` |
| branch | `fix/browse-neighbour-prefetch`, cut from `origin/dev` (`f97268eba7`) |
| commits | 6 — the three transplanted below plus three review fixes |
| Android gate | **green** — `suites=86 tests=487 failures=0 errors=0 verdict=fresh` |
| review | **done** — two reviewers, four findings accepted and applied by Codex |
| pushed | **no** |

**§1 below has its ahead/behind backwards.** `origin/dev...HEAD` printed `9 3`,
which is 9 *behind* and 3 *ahead* — the branch carried three commits, not nine.
That inverts §8's first instruction.

**The branch was not rebased, and should not be.** Its third commit,
`bbdc00e32c` (contrast), is superseded on dev by a further-evolved version via
PRs #710 and #713 — dev's `panel_contrast.rs` is the same file grown past it.
Rebasing meant resolving contrast conflicts for a commit that must not land.
The contrast commit touches no Android file, so the Android work never depended
on it. Instead: fresh worktree off `origin/dev`, `git cherry-pick d387747bf0
48e6138b0f`, the uncommitted §2.2 carried across as a patch. Clean, no
conflicts — dev's only Android change in those nine commits was two lines in
`build.gradle.kts`.

The message of `48e6138b0f` was rewritten during the transplant. §4 documents
two false claims in it; squash merges take the subject verbatim into dev, so
that was the last cheap moment to fix it.

### What the second session found on top

**A latent race, not one flake.** The transplanted tree failed
`ComposeBehaviorTest.aScanLeavesNoTabAnsweringWithTheWholeLibraryWhileTheQueryStands`
on the first gate run and passed on the next two — the same unfixed code, green
twice. Rerunning proves nothing here. Raising `NEIGHBOUR_PREFETCH_IDLE_MS` to
60 s, where no looper can idle past the wait, made it deterministic: **four**
failures without the fix (that test, two `MainActivityConfigurationTest`
recreation tests, and the neighbour test), **one** with it — the neighbour
test, which cannot outlast a 60 s wait by construction. See
[[a-green-android-gate-does-not-disprove-a-timing-race]]. So the green gate
§1 reports was a won race, not evidence.

The cause: the prefetch `LaunchedEffect` was keyed on `pendingTab` but not on
`selectedTab`. Selecting a tab whose prefetch is already waiting leaves
`pendingTab` unchanged, so the effect never re-entered and the wait ran on
under a tab someone was looking at — a tap right after a search paid the full
idle period. Fixed by adding the key.

**Review findings applied** (Codex, in the same worktree):

1. `runCatching` swallowed `CancellationException`. Before this branch the
   block held no suspension point; `withContext(Dispatchers.IO)` introduced
   one, and `withContext` rethrows on resumption when the job was cancelled —
   even after the inner call succeeded. `.onFailure` then wrote a real error
   banner ("Could not load artists: JobCancellationException") for a fetch that
   did not fail, and because `browseError = null` is gated on `visible`, a
   later silent prefetch could not clear it. Now rethrown.
   `CancelledBrowseDoesNotReportFailureTest` blocks the artist read on a latch,
   navigates away, releases it, and asserts no banner — verified to fail
   without the rethrow.
2. The test replacing #371's rule asserted that the *window was requested*, not
   that the rows landed: deleting `visibleArtists = it` kept it green. It now
   asserts a rendered artist row, against a test application whose initial
   state carries no artist rows. Verified by deleting that line and watching it
   fail.
3. The test's opening assertion that the prefetch had *not yet* fired was
   clock-order-dependent — measured in this class, a 400 ms delay had already
   elapsed by the time a `performClick()` returned. Dropped; it was never what
   the test existed to prove.
4. `summary` was a fresh lambda per recomposition while `shownTab` was
   memoised, defeating the skipping the lambda was introduced for. Now
   `remember`ed over the seven values it reads.

**Reviewed and deliberately not fixed.** The main thread can now block on the
Rust `Mutex<Db>` behind a background prefetch, because `search()`, the
`loadMore*` functions and `openAlbumDetail` still run synchronously on the
calling thread; and a cancelled prefetch's blocking JNI call cannot be
interrupted, so a newly selected tab can queue behind it. Both are the same
root and belong to §6's third open item, with its own measurement. Bounded by
query duration, not unbounded.

**Still open:** nothing is pushed, no PR exists. The shared checkout still
carries the pre-transplant `BrowseScreen.kt` and `MobileBottomTabsTest.kt` as
dirty files on `fix/contrast-button-fixes` — a backup, not live work. Discard
them once this branch is pushed.

---

## 1 — State at handover (first session — superseded, see §0)

| | |
|---|---|
| branch | `fix/contrast-button-fixes` |
| checkout | `~/Projects/reprise` (shared, not a worktree) |
| HEAD | `48e6138b0f` |
| vs `origin/dev` (`f97268eba7`) | 9 ahead, **3 behind** — not rebased |
| uncommitted | `BrowseScreen.kt`, `MobileBottomTabsTest.kt` (`+93/−21`) |
| Android gate | **green** — `suites=85 tests=486 failures=0 errors=0 verdict=fresh` |
| `/check` | **not run** |
| device | Pixel 10 Pro XL, release build of the final tree installed |

The branch is **behind `origin/dev` by 3** and was never rebased. Do that before
anything else — see [[survey-against-dev-not-local]].

### Commits on the branch that touch this work

```
48e6138b0f  fix(android): move blocking JNI calls to IO dispatcher for first-Titles stutter
d387747bf0  fix(android): smooth tab switching with real-time pill and gated counts
```

**Neither was made by the session that did the work.** `d387747bf0` is this
session's change, committed from elsewhere at 09:09. `48e6138b0f` came from
elsewhere entirely at 09:10 — read §4 before trusting its message.

---

## 2 — What was wrong, and what is fixed

### 2.1 The mark stood still — fixed, committed as `d387747bf0`

`BrowseScreen.kt` drove `selectedTab` from `pagerState.settledPage`, which keeps
its old value for the whole drag *and* the whole fling. The navigation bar
(`LibraryFrame.kt`, `selected = destination == selectedTab`) and the header
count therefore could not move until everything came to rest.

Now both read `pagerState.targetPage` — the page the gesture is committed to —
through a `() -> BrowseTab` handed down so the read lands in the item's own
recomposition scope, not in the 737-line browse screen.

`selectDestination` deliberately **stays** on `settledPage`. It nulls
`selectedAlbum`/`selectedArtist` and calls `showNowPlaying(false)`; fired
mid-drag, an aborted swipe would wipe detail state for a tab nobody left. Its
`drop(1)` matters too — it is what stops the first composition from nulling the
rehydrated `restored?.openAlbum`.

The count line is additionally gated on the marked tab being loaded. Without
that gate it printed `0 of 65 artists loaded` mid-swipe — a *false* number where
the old behaviour showed a stale-but-true one. Verified by cropping the summary
row out of a screen recording frame by frame.

### 2.2 The tab next door was drawn empty — fixed, **uncommitted**

`LibrarySession.browseState` fills rows for the tab the library opens on and
hands every other tab back through `withoutRows()`: a total, no rows. A swipe
draws the next page as it begins and settles afterwards, so the first swipe onto
a tab showed an empty list and filled it on landing. This is what the user
actually reported; §2.1 does not address it.

`BrowseScreen.kt` now prefetches. `pendingTab` picks the visible tab first, then
whatever is still unfetched; the fetch runs in `Dispatchers.IO` and hands a
`LoadedTab` back for assignment. A prefetch waits for
`NEIGHBOUR_PREFETCH_IDLE_MS` (400 ms) **and** `!pagerState.isScrollInProgress`,
so it never competes with the opening frames or with a gesture. A prefetch stays
silent: it must not clear an error the visible tab is showing, nor raise one for
a tab nobody asked for.

**This reverses a deliberate rule.** PR #371 (2026-08-08) locked in
`aComposedNeighbourRequestsNoWindowUntilItBecomesTheVisibleDestination`. That
test was **replaced, not deleted**, by
`aNeighbourIsFetchedWhileTheScreenIsStillSoNoSwipeLandsOnAnEmptyList`, whose
doc comment carries the reasoning. The window is a bounded 200 rows either way;
what the old rule saved was one such query, what it cost was every first swipe.
If that trade is wrong, revert both the effect and the test together.

---

## 3 — Rejected, with evidence

**`beyondViewportPageCount = 1` on the `HorizontalPager`.** Composes the
neighbour ahead of the drag; looked like the obvious smoothing fix. It broke
**10 tests** in the full suite — recreation tests in
`MainActivityConfigurationTest` plus `MainActivityQueueTest`, all
`<node> is not displayed`, because the off-screen page is composed but placed
outside the viewport. It also revives `NowPlayingQueuePage`, whose
`LaunchedEffect(playback.currentTrackId) { reload() }` then refetches the queue
on every track change while invisible.

Isolation: full suite **10 failures with the slack, 0 without**, nothing else
changed. Do not reach for it again — see
[[pager-slack-composes-the-queue-off-screen]].

---

## 4 — Read this before trusting `48e6138b0f`

Two claims in that commit's message do not hold:

1. *"The state mutations happen back on the main dispatcher automatically after
   the IO call completes."* They do not. The assignments sat **inside** the
   `withContext(Dispatchers.IO)` block, so they ran on the IO thread. It works
   only because Compose snapshot state tolerates writes from any thread. The
   uncommitted change in §2.2 restructures this so the fetch is in IO and the
   handover to Compose is not.
2. *"Fixes: Stutter on first Titles display after tapping the tab."* No such
   stutter was measurable. First tap onto Titles from a settled Queue start:
   **0 janky frames, p99 11 ms, 0 slow UI thread**; second tap identical. The
   library is 741 titles, so the blocking query is too fast to see.

The device was running the 00:34 build throughout the user's testing, which
does **not** contain `48e6138b0f`. Nothing observed on the device that day says
anything about that commit.

---

## 5 — Measurements, and how to repeat them

All on a Pixel 10 Pro XL, 120 Hz (`vsyncRate=120.00 Hz`, ARR, render range
0–120 Hz), so the frame budget is **8.33 ms**.

### The build is the dominant variable

| arm | tap p99 | swipe p95 | swipe p99 | swipe jank |
|---|---|---|---|---|
| debug | 85 ms | 20 ms | 133 ms | 3.32 % |
| release | 16 ms | 7 ms | 16 ms | 1.22 % |

Both on unmodified baseline code. **Never conclude anything about frame times
from a debug build** — see [[android-debug-build-frame-times-are-worthless]].

### The fix is perf-neutral

Release, final tree: startup 843/812/818 ms against 839/844/837 ms before the
prefetch; swipe 1.34 % jank / p95 7 ms against 1.21 % / p95 8 ms. One run per
arm — treat these as "no regression", not as an improvement.

### The desync, measured

Screen recording at 10 fps through a 1000 ms synthetic swipe, per-frame
brightness of each navigation slot to find which pill is lit:

- baseline: mark leaves Titles at frame 10, lands frame 11
- fixed: leaves at frame 3, lands frame 4

≈600–700 ms of desync removed. On **tap**, mark and page move in the same 50 ms
sample (f057 → f058) — no latency was traded away on the path people use most.

### Repeating it

Scripts lived in the session scratchpad and are **gone** — the scratchpad is
tmpfs and was reclaimed mid-session (see [[agent-tmp-gc-reclaims-tmpfs]]).
Rebuild them from this recipe:

- Nav item centres: `uiautomator dump`, take `bounds` of nodes with
  `text="Titles|Artists|Queue"` and `y > 1800`. On this device: 172 / 539 / 906
  at y 2292.
- Frame times: `dumpsys gfxinfo <pkg> reset`, drive the gesture, then
  `dumpsys gfxinfo <pkg>`. Works fine on a non-debuggable release build.
- Pill position: `screenrecord`, `ffmpeg -vf fps=10`, mean luminance of a
  ±110 px band around each centre in the bar row; the lit slot is the brightest.
- The summary row crops at `(20,190)–(860,250)`.
- **Wrap every `adb` call in `timeout`.** The phone dropped off USB twice and
  un-timed-out calls hang the whole run.
- **Filter Choreographer by pid.** `logcat -s Choreographer:*` catches every
  process on the device; an unfiltered read produced `Skipped 352307 frames`,
  which belonged to something else entirely.

---

## 6 — Open, deliberately not touched

**Cold start spends one frame of 150–250 ms**, reproducible 3/3, with
`TotalTime` ≈ 840 ms. `MainActivity.onCreate` runs three synchronous JNI+SQLite
queries via `restoreLibrary` *before* `setContent`, and the project has **no
baseline profile at all** (`find app/src -iname "*baseline*"` is empty), so the
first composition runs without profile-guided AOT. A baseline profile is the
standard fix and is the next real lever. It is build infrastructure — a
benchmark module — and was out of scope here.

**`LibraryTrackRows.kt:96` does `order = content.joinToString(...)` over the
whole loaded window on every recomposition** of `TrackRows`, and the window
grows with each `loadMore`. O(n) main-thread work that recurs; it explains no
reported symptom but will hurt on large libraries.

**Moving the rest of the library calls off the main thread.** Every
`LibrarySession`/`AndroidLibrarySessionPort` call — `openAlbum`, `openArtist`,
`listArtistTracks`, the click handlers — still runs synchronously on whatever
thread calls it. `ArtistPortraitPrefetch` already drives the same FFI handle
from its own executor, so the Rust-side `Mutex<Db>` tolerates it; but
`LibrarySession.scanMonitor` guards `chooseTree`/`rescan`/`autoScan` with a
Kotlin-level lock that the query calls do **not** take, so a background query
racing a scan is gated only Rust-side. Not validated.

---

## 7 — Working state you did not create

`scripts/android-build.sh` was run with `ANDROID_HOME=~/.local/share/android-sdk
ANDROID_TARGET=aarch64-linux-android ANDROID_ABI=arm64-v8a ANDROID_API=26`. It
overwrote `android/app/src/main/jniLibs/arm64-v8a/libreprise_android_ffi.so` and
regenerated `android/app/src/main/java/uniffi/**`. Both are gitignored, so
`git diff` will not show them.

This was necessary, not incidental: the `.so` on disk only understood **schema
79** while the device database is at **80** (commit `3190c75f28`), so every
release build crashed on launch with
`LibraryException$Database: database schema 80 is newer than supported schema 79`.

Note also that `android/keystore.properties` is absent, so `hasReleaseSigningConfig`
is false and the release build is signed with the **debug** key. That is why it
installs over an existing debug install without losing library data — and why it
is not a distributable artifact.

---

## 8 — Next

1. Rebase onto `origin/dev` (3 behind) and re-run
   `scripts/check-android-suite.sh` with `ANDROID_HOME` set — the bare
   `./gradlew :app:testDebugUnitTest` path fails with
   `NoClassDefFoundError: UniffiLib` because it skips host binding generation.
   Filtering with `--tests` is also unsafe here: three classes run in isolation
   report 5 failures that the full suite does not.
2. Commit §2.2 (`BrowseScreen.kt`, `MobileBottomTabsTest.kt`). Say in the message
   that it reverses #371's neighbour rule.
3. Decide whether §2.2's trade is wanted at all; it is the one judgement call in
   this work that belongs to the owner.
4. `/check` has not run on any of it.
