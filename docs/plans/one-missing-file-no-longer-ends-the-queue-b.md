---
slug: one-missing-file-no-longer-ends-the-queue-b
worktree: /home/marvin/Projects/reprise-one-missing-file-no-longer-ends-the-queue-b
branch: feature/one-missing-file-no-longer-ends-the-queue-b
phase: shipped
codex_session:
created: 2026-09-01
---
# Strand B — The fault path says what actually happened

Mother plan: `docs/plans/one-missing-file-no-longer-ends-the-queue.md`. Read it
and `docs/plans/android-source-error-on-synced-track.findings.md` first.

Diagnosing the photographed bug took an afternoon of device forensics because
the app throws away the one thing that would have named it. This strand is
small, self-contained, and Kotlin only.

## File ownership

Touch only:

- `android/app/src/main/java/de/reprise/spike/Media3PlaybackPort.kt`
- `android/app/src/test/java/de/reprise/spike/**`

Touch no Rust — strand A owns the Rust playback path, including what the user
finally sees in the banner.

## The constraint that shapes this strand

**The enriched text goes to logcat, not to the UI** (mother plan, D5). Android's
only error surface is `BrowseErrorLine`, fed from `snapshot.error`, and strand A
fills that with FB-6's sentence. Putting `FileNotFoundException` there would be
developer text in a user interface and would diverge from an active UX rule.

The message this strand builds still travels in `AndroidPlayerEvent.Error` —
strand A logs it via `tracing` rather than displaying it — and is additionally
written straight to logcat here. Both paths are wanted: the `Log.e` carries the
stack, the event message carries the summary.

## B1 — Walk the cause chain

`Media3PlaybackPort.kt:91`:

```kotlin
override fun onPlayerError(error: PlaybackException) {
    val detail = error.message ?: error.errorCodeName
    emit(AndroidPlayerEvent.Error("${error.errorCodeName}: $detail"))
}
```

`error.message` for a source-type `PlaybackException` is the constant string
`"Source error"` — identical for a missing file, a lost permission and a corrupt
container. Walk `error.cause` to the root and append each link's simple class
name and message, so the summary reads like

```
ERROR_CODE_IO_UNSPECIFIED: Source error — FileNotFoundException: No such file or directory
```

Bound it, because this string crosses FFI and reaches a log:

- cap the chain at 3 links;
- guard against a self-referential or cyclic `cause` (compare identity, do not
  trust the chain to terminate);
- cap total length so a pathological message cannot flood the log;
- tolerate a null message on any link.

## B2 — Log it with the stack

Nothing is logged today. Add `Log.e(TAG, "…", error)` passing the throwable, so
the full stack reaches `adb logcat`. `TrackCover.kt` already establishes the
`TAG` idiom in this package — match it rather than inventing another.

One line per fault. Do not log on every retry or state change, and keep the
track URI out of any level that would make a normal session noisy.

## B3 — A unit test on the message builder

Extract the chain walking into a small pure function — top-level or a private
helper reachable from tests — so it can be exercised without a `Player`
instance. `android/app/src/test/java/de/reprise/spike/` already hosts this kind
of test (`BrowseSurfaceTest.kt`, `MobileHeaderRowTest.kt` and friends).

Cover: no cause; exactly one cause; a chain deeper than the cap; a link whose
message is null; a cyclic chain; and a pathologically long message hitting the
length cap.

## Verification

```sh
scripts/check-android-suite.sh
```

or, if it is meaningfully faster, the narrower Gradle unit-test task that script
wraps. Both stay inside this strand's ownership.
