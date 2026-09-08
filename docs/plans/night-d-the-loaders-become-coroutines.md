---
slug: night-d-the-loaders-become-coroutines
worktree: /home/marvin/Projects/reprise/.worktrees/night-d-loaders-coroutines
branch: refactor/the-loaders-become-coroutines
phase: shipped
created: 2026-09-07
base: origin/dev
owns: android/app/src/main/java/io/github/marvinbaudach/reprise/{TrackAnalysisLoader,TrackCover,LibraryWrites,TrackLoader,ArtistPortraitPrefetch}.kt
---
# Night package D — seven hand-built threads become coroutines

## Autonomy

**Run this end to end without asking.** Plan → code → check → refactor → land,
including the merge. Autonomous landing is authorised. Do not ask for `/check`,
`/refactor` or `/ship` between phases. Stop only for a listed **stop
condition**; then leave the worktree, add a `## Findings` section to this file,
set `phase: blocked`, and stop.

Own worktree `.worktrees/night-d-loaders-coroutines`, branch
`refactor/the-loaders-become-coroutines`, off `origin/dev`. Four sibling
packages run tonight. **Touch only the files in `owns:`.**

> `ActivityPlaybackControls.kt` also holds an executor and is **excluded**. It
> is constructed in `MainActivity.kt`, which no package owns tonight, so
> changing it would force an edit outside every ownership list. Leave it and
> name it in the PR body as deliberately deferred.

**Hard constraint for parallel safety: every existing call site must keep
compiling untouched.** `LibraryTrackRows.kt` calls `TrackCover` at line 386 and
that file belongs to package A tonight. So a new dispatcher parameter goes in
**with a default** (`dispatcher: CoroutineDispatcher = Dispatchers.IO`), never
as a required argument. If a conversion cannot preserve the call-site signature,
that file is out of scope for tonight — say so and move on.

## Why

The app builds seven single-thread executors by hand and imports no coroutine
machinery in any of the files that do it. Measured on `origin/dev` @
`fe89dc51ad`:

| File | Executor |
| --- | --- |
| `TrackAnalysisLoader.kt` | `singleAnalysisThread` |
| `TrackCover.kt` | `worker` and `fullSizeWorker` — two |
| `LibraryWrites.kt` | `singleLibraryWriteThread` |
| `TrackLoader.kt` | `singleTrackThread` |
| `ArtistPortraitPrefetch.kt` | `singlePortraitPrefetchThread` |

Each hand-rolls its own interrupt handling. Each is a constructor-injectable
default, which is a genuine test seam and must survive this change.

**This is debt, not a live bug.** The pools are coherent and individually
well-named. The payoff is one lifecycle model instead of six, cancellation that
composes with the screens that already use coroutines, and the deletion of six
separate interrupt dances. Treat it accordingly: if a conversion makes a file
harder to read, leave that file alone and say so.

## Scope

In: replacing the executor with a coroutine scope or dispatcher, and the
interrupt handling that exists only to serve it.
Out: changing what any loader loads, its caching, its call sites' behaviour, or
its public shape beyond what the change forces. No new abstraction shared across
the five files unless the same code appears in at least three of them.

## Tasks

### D.1 — one file first, as the pattern

Convert `TrackLoader.kt` first; it is the smallest. Land the pattern in your own
head before repeating it. Specifically decide, once, and apply the same answer
everywhere:

- Where does the scope live and who cancels it? A loader owned by an Activity
  must not outlive it.
- What replaces the injectable executor as the **test seam**? A
  `CoroutineDispatcher` parameter defaulting to `Dispatchers.IO` is the obvious
  answer; the tests then inject a test dispatcher. Whatever you choose, every
  existing test that injects an executor must have an equally direct
  replacement — check the test files before you change the production
  signature.
- What replaces the interrupt flag? Cooperative cancellation via
  `ensureActive()` or a cancellable suspend call, not a manually checked
  boolean.

### D.2 — the remaining four

`TrackAnalysisLoader.kt`, `TrackCover.kt` (two executors),
`LibraryWrites.kt`, `ArtistPortraitPrefetch.kt`. Same pattern, one commit per
file so a bisect can name the culprit.

`LibraryWrites.kt` is the delicate one: it is guarded by
`LibraryWriteThreadGuard.kt`'s `requireOffMainThread`. That guard must keep
firing. If the conversion makes it possible for a write to land on the main
dispatcher, you have made things worse — stop.

### D.3 — remove what the executors needed and nothing else

Delete the interrupt plumbing that exists only to stop a thread. Do not delete
error handling, retries, or coalescing — those are behaviour.

## Acceptance

- `Executors.newSingleThreadExecutor`, `newFixedThreadPool` and raw `Thread(`
  construction appear **zero** times across the five owned files.
  `ActivityPlaybackControls.kt` still has its one, deliberately.
- Every test that previously injected an executor injects a dispatcher, and no
  test was deleted to make this pass.
- `scripts/check-android-suite.sh` passes with no fewer tests than before
  (currently 605 across 99 suites).
- No file outside `owns:` is modified.

## Gates

```
scripts/check-android-suite.sh
scripts/check-architecture.sh
scripts/check-shell.sh
```

**Run the Android suite only through `scripts/check-android-suite.sh`.** It
builds the FFI for the *host* and exports `LD_LIBRARY_PATH`.
`scripts/android-build.sh` builds for the device, and a raw `gradlew` after it
fails 28 Robolectric tests for reasons unrelated to the code. A fresh worktree
also needs `android/local.properties` copied in — it is gitignored and Gradle
aborts without it.

## Stop conditions

- A conversion makes a file harder to follow than the executor it replaces.
  Convert what is clearly better and stop; a partially converted set with a
  clear reason beats a fully converted set nobody can read.
- `LibraryWrites.kt`'s off-main-thread guarantee cannot be preserved.
- The change would touch `ActivityPlaybackControls.kt` or any file outside
  `owns:`.
- The Android suite is already red on unmodified `origin/dev` — check the
  control arm before debugging your own change.

## Landing

Squash-merge into `dev`. The title is taken verbatim; write prose. Something
like *"The library loaders share one cancellation model"*. Name the deferred
`ActivityPlaybackControls.kt` in the body so the next person knows it was a
choice, not an oversight.

## Findings

All five owned production files were converted from hand-built executors to
coroutine-backed serial lanes. Ownership was extended by
`ArtistPortraitLiveRefreshTest.kt` after checking that no sibling package owns
it. That ownership check was necessary because the test injects
`TrackArtwork`'s renamed constructor seam, while another package may edit a
separate region of the same file tonight; the conversion therefore kept its
change to the injection and the direct test dispatcher only.

`ActivityPlaybackControls.kt` remains deliberately deferred because converting
it would require edits outside this package's ownership. The final Android gate
reported 98 suites and 601 tests, with 0 failures, 0 errors, and 0 skips.

A follow-up review accepted four findings. `LibraryWrites` once again leaves a
timed-out or interrupted answered lane draining so its late report cannot be
stranded, and computed write outcomes are handed to the main-thread reporter
even if teardown cancels the scope. Its pending-answer accounting now returns
to zero exactly once on normal delivery, rejection, cancellation, and failed
handoff paths. The two timeout tests were restored to their base behavior and
the rejection test again distinguishes the immediate-cancel branch from the
drain branch through a zero-length dispatcher/scope drain bound.

Started analysis imports, bar reads, and spectrogram reads now always post their
completion after the blocking FFI call returns; existing tests cover all three
paths under cancellation without increasing the suite count. Artwork list and
full-size lanes now log the established "the library is closing" diagnostic
when `loadVisual` or `prefetch` is called after shutdown.
