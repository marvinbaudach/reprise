---
slug: night-a-android-reads-leave-the-main-thread
worktree: .worktrees/night-a-android-reads
branch: feature/android-reads-leave-the-main-thread
phase: planned
created: 2026-09-07
base: origin/dev
owns: android/app/src/main/java/io/github/marvinbaudach/reprise/{MainActivitySurface,BrowseScreen,LibraryScreen,BrowseTabs,LibraryTrackRows}.kt
---
# Night package A — library reads leave the main thread

## Autonomy

**Run this end to end without asking.** Plan → code → check → refactor → land,
including the merge. The owner has explicitly authorised autonomous landing for
this package and does not want to be asked to type `/check`, `/refactor` or
`/ship` between phases. The only reasons to stop are listed under **Stop
conditions**; if one fires, leave the worktree in place, write your findings
into this file under a `## Findings` heading, set `phase: blocked`, and stop.

Work in your own worktree (`.worktrees/night-a-android-reads`, branch
`feature/android-reads-leave-the-main-thread`, branched from `origin/dev`).
Four sibling packages run in parallel tonight; **touch only the files listed in
`owns:` above.** If the work seems to require a file outside that list, that is
a stop condition, not a licence.

> `LibraryTrackRows.kt` calls into `TrackCover`, which package D owns tonight.
> Treat `TrackCover`'s API as fixed: D is under instruction to keep every
> call-site signature compiling unchanged, so you should never need to edit that
> call. If you do, stop — the two packages have collided and that needs a human.

## Why

`BrowseScreen.kt`'s background prefetch wraps its FFI query in
`withContext(Dispatchers.IO)` and explains why in a comment: *"The rows come off
a blocking JNI + SQLite call; only the handover to Compose belongs on the main
thread."*

The pagination lambdas a few lines below call the **same** functions with no
such wrapper, driven by `LaunchedEffect`, which runs on the composition's main
dispatcher. The whole file contains two `withContext` occurrences.

Affected paths, all measured on `origin/dev` @ `fe89dc51ad`:

| Call site | What the user is doing |
| --- | --- |
| `loadMoreTitles`, `loadMoreArtists` | scrolling a list to its end |
| `loadMoreAlbumTracks`, `loadMoreArtistTracks`, `loadMoreArtistAlbums` | scrolling inside an album or artist |
| `search(text)` | typing in the filter field |
| `openAlbumDetail` | tapping an album |

Those are the three most-touched interactions in the app, and every one of them
runs a blocking JNI plus SQLite call on the thread that draws frames.

**The cause is one level up, and that is where the fix belongs.**
`MainActivitySurfaceDependencies` declares its query fields as plain
value-returning functions. A plain function gives every call site the *option*
to forget dispatching, and most of them did. Making the seam `suspend` removes
the option: the compiler then refuses a call outside a coroutine, so a future
call site cannot reintroduce the bug.

Fixing the seven call sites individually would leave the shape that produced
them. Do not do that.

## Scope

In: the query seam and the call sites that break because of it.
Out: any new caching, any change to what is queried, any UI redesign, any change
to the prefetch that already works — it is the reference implementation, copy it.

## Tasks

### A.1 — a failing test first

Before touching production code, add a test that fails for the current
behaviour. The suite already has the tools: Robolectric plus Compose test rules,
see `android/app/src/test/java/io/github/marvinbaudach/reprise/` for the idiom.

The honest assertion is "a library query does not run on the main thread".
Implement it by making the fake query lambda in the test record
`Thread.currentThread()` (or assert `Looper.getMainLooper().thread` is not the
caller) and driving a pagination request through the screen. There is a
precedent for a thread guard in `LibraryWriteThreadGuard.kt`'s
`requireOffMainThread` — read it, and prefer reusing that guard over inventing a
second one.

Confirm the test is red against unmodified code before continuing. A test that
passes before the fix is not evidence.

### A.2 — make the seam suspend

In `MainActivitySurface.kt`, change the query fields of
`MainActivitySurfaceDependencies` to `suspend` functions. Do not add
`withContext` inside every implementation: put the dispatch in **one** place, at
the seam, so a reader can see the rule once. Follow the wording of the existing
prefetch comment — the handover to Compose is what belongs on the main thread,
the query is not.

### A.3 — carry the change through the call sites

`BrowseScreen.kt`, `LibraryScreen.kt`, `BrowseTabs.kt`, `LibraryTrackRows.kt`
will stop compiling. That is the point: each error is a call site that was
silently on the main thread. Fix them by moving the call into the coroutine that
`LaunchedEffect` already provides, not by wrapping each one in its own
`withContext`.

Delete the now-redundant `withContext(Dispatchers.IO)` in the prefetch if the
seam already dispatches — two dispatches for one call is worse than none,
because it suggests the seam cannot be trusted.

### A.4 — the guard stays

`LibraryWriteThreadGuard.kt` currently guards writes only. If A.1 reused it,
reads are now guarded too and a regression fails a test rather than a stopwatch.
Say so in the commit message.

## Acceptance

- The test from A.1 passes, and you have observed it fail before the fix.
- `scripts/check-android-suite.sh` passes with no fewer tests than before
  (`ANDROID_TEST_FLOOR` in that script is the floor; the current measured count
  is 605 across 99 suites).
- `grep -c withContext android/app/src/main/java/io/github/marvinbaudach/reprise/BrowseScreen.kt`
  is **lower** than today's 2, not higher — the dispatch moved to the seam
  instead of being sprinkled.
- No file outside `owns:` is modified.

## Gates

Run all of these; all must pass before landing.

```
scripts/check-android-suite.sh          # host FFI + the JVM suite
scripts/check-architecture.sh
scripts/check-shell.sh
```

**The Android suite runs only through `scripts/check-android-suite.sh`.** It
builds the FFI for the *host* and exports `LD_LIBRARY_PATH`.
`scripts/android-build.sh` builds for the device instead; a raw `gradlew` run
after it fails 28 Robolectric tests for reasons unrelated to any code change. A
fresh worktree also needs `android/local.properties` copied in — it is
gitignored, and Gradle aborts before compiling without it.

## Stop conditions

Stop, record findings, do not land, if:

- The change requires editing a file outside `owns:`. A sibling package may own
  it tonight and your merge would conflict.
- Making the seam `suspend` forces a change to the FFI surface or to
  `reprise-android-ffi`. That is a different package.
- The Android suite is already red on unmodified `origin/dev`. Check the control
  arm before debugging your own change; a red base is not yours to fix here.
- The test from A.1 cannot be made to fail against unmodified code. That means
  the bug is not where this plan says it is, and the plan is wrong. Say so.

## Landing

Squash-merge into `dev` with a title that says what changed for the user, not
what was refactored. The PR title is taken verbatim by the squash merge, so it
must read as English prose. Something in the shape of *"Scrolling and typing no
longer wait for the database"*.
