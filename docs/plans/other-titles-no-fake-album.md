---
slug: other-titles-no-fake-album
worktree: /home/marvin/Projects/reprise-other-titles-subtitle
branch: fix/other-titles-no-fake-album
phase: shipped
codex_session:
created: 2026-08-13
---
# "Other titles" must not say "Unknown album"

## Why this exists

The artist page (shipped in #437) lists an artist's album-less tracks under an
"Other titles" heading. Seen on an emulator on 2026-08-13, each of those rows
carries the subtitle **"Unknown album"**.

That contradicts the decision the feature was built on. From the design spec: a
track with no album gets **its own section** *rather than* being hidden or folded
into a fake "Unknown album". The section does that correctly — but the row text
then says exactly the phrase the decision rejected, on the one screen where the
heading above it already says these tracks have no album.

Fix: on that screen, such a row gets **no subtitle line at all**.

## The change

One file, one expression.

`android/app/src/main/java/de/reprise/spike/LibraryTrackRows.kt:288-299` renders
the subtitle unconditionally:

```kotlin
Text(
    text = when (subtitle) {
        TrackRowSubtitle.ARTIST_AND_ALBUM -> track.details()
        TrackRowSubtitle.ALBUM_ONLY -> track.album.ifBlank {
            "Unknown album"
        }
    },
    style = MaterialTheme.typography.bodyMedium,
    color = MaterialTheme.colorScheme.onSurfaceVariant,
    maxLines = 1,
    overflow = TextOverflow.Ellipsis,
)
```

Make the subtitle nullable and emit the `Text` only when there is one. Shape —
adapt to the surrounding style, do not copy blindly:

```kotlin
val subtitleText = when (subtitle) {
    TrackRowSubtitle.ARTIST_AND_ALBUM -> track.details()
    TrackRowSubtitle.ALBUM_ONLY -> track.album.ifBlank { null }
}
if (subtitleText != null) {
    Text(
        text = subtitleText,
        // …unchanged style, colour, maxLines, overflow…
    )
}
```

`"Unknown album"` disappears from the file. It is a hardcoded Kotlin literal at
that one spot, not a string resource, so nothing else needs updating.

### Why this is safe to scope to one branch

`TrackRowSubtitle.ALBUM_ONLY` has exactly **one** caller: the artist page's
"Other titles" list at `BrowseTabs.kt:375`. Every other surface — the Titles tab
(`BrowseTabs.kt:65`), the album page (`BrowseTabs.kt:160`) and the queue
(`NowPlayingQueue.kt:94`) — passes `ARTIST_AND_ALBUM`, whose branch this change
does not touch. Verify that claim with a grep before you edit; if a second
`ALBUM_ONLY` caller exists, stop and say so, because then the blank-album row
appears somewhere the heading does not explain it.

An `ALBUM_ONLY` row whose album is **not** blank keeps showing the album name.
Only the blank case loses its line.

### The row does not get shorter

`LibraryTrackRows.kt:238` pins the row to `metrics.trackRowHeightDp` (72 dp
regular, 64 dp compact — `LibraryFramePolicy.kt:16,24`). Dropping the second line
therefore leaves the row height alone; the title simply centres. Do not add
padding or a spacer to "compensate" — there is nothing to compensate.

## Tests

No test asserts on `"Unknown album"` today, in Kotlin or in Rust — the Rust
occurrences (`crates/reprise-view/src/strings/browse.rs`,
`crates/reprise-gnome/.../browse_bar_tests.rs`) belong to the desktop browse bar
and are unrelated. **Do not touch the Rust side.**

Add coverage to the artist-page surface test
(`android/app/src/test/java/de/reprise/spike/ArtistDetailSurfaceTest.kt`), in that
file's existing style:

1. An artist with an album-less track: the "Other titles" row shows the title and
   **no** "Unknown album" text anywhere on the screen.
2. **A control in the same test or file**: an `ALBUM_ONLY` row whose album is set
   still displays that album name. Without this, deleting the subtitle entirely
   would also pass, and the test would be proving nothing.

Assert on absence with the node matcher the file already uses for "this text is
not displayed"; do not assert only on the presence of the title.

## What must not change

- Behaviour of `ARTIST_AND_ALBUM` on any surface.
- Row height, padding, cover size, the heart, the play-count pill, the duration.
- The Rust `UNKNOWN_ALBUM` string and its desktop users.

**The file list above is a starting point, not a fence.** If the edit needs a
minimal adjustment in an adjacent file to compile, make it and name it in the
commit message. Stop only if the *contract* is wrong.

## Verification

From the worktree root. Redirect long output to a log and grep it rather than
reading it whole.

```bash
scripts/check-architecture.sh
scripts/check-frontend-thinness.sh

export JAVA_HOME=/usr/lib/jvm/java-21-openjdk
export ANDROID_HOME="$HOME/.local/share/android-sdk"
cd android && ./gradlew --max-workers=2 \
  -Pkotlin.compiler.execution.strategy=in-process \
  :app:testDebugUnitTest
```

**JDK 21 is mandatory** — JDK 26 breaks Robolectric.

Gradle prints `BUILD SUCCESSFUL` for a task that ran no tests at all. The
evidence is the XML: count the files and sum the attributes under
`android/app/build/test-results/testDebugUnitTest/TEST-*.xml`. Quote the totals.
Before #437 landed the suite was 66 suites / 333 tests / 0 failures; your added
tests should raise the test count and nothing else.

This change touches no Rust, so no `cargo` command is part of this task. Do not
record a baseline and do not run `cargo test --workspace`: under the Codex
sandbox (`workspace-write`) the `reprise-core` cover tests fail with
`ReadOnlyFilesystem` because they write to `dirs::cache_dir()`, and `cargo audit`
cannot lock its advisory database. Both are sandbox limits, not regressions.

## Done when

- `"Unknown album"` no longer appears in production code under
  `android/app/src/main`. A test that asserts the phrase is *absent from the UI*
  must of course still name it, plainly — do not split or obfuscate the literal
  to satisfy a grep.
- An album-less row on the artist page renders one line; an `ALBUM_ONLY` row with
  an album still renders its album name, proven by a test.
- Both gate scripts exit 0 and the Android suite is green, with the XML totals
  quoted as evidence.
- `git diff` shows the one composable change plus the new tests.
