---
slug: android-ux-pass
worktree: /home/marvin/Projects/reprise-android-ux-pass
branch: feature/android-ux-pass
phase: planned
codex_session:
created: 2026-10-01
---
# Android UX pass — centred mini player, honest counts, one artist count line

Findings from a device tour on 2026-10-01 (Pixel 10 Pro XL, app 0.1.164), grilled the same day.
Android only: Kotlin/Compose under `android/app/src/{main,test}/java/io/github/marvinbaudach/reprise/`
(below: `main/` and `test/`). No Rust, no `reprise-core`/`reprise-view` change.

## Binding rules and specs

- `docs/ux-rules.md` has no `[android]` rule. MINI-1…6 are the GTK compact window and PLAY-13 is
  the GTK player bar; nothing pins the Android mini player alignment, the plural wording, the
  `--:--` placeholder or the artist detail chrome. Following #988 (Android polish without a rule),
  **this plan adds no rule** — decided in the grill. Do not edit `docs/ux-rules.md`.
- `docs/superpowers/plans/2026-08-04-mobile-m3.md:52-54` (design 2a) built the frame: "Mini player
  72 dp, radius 16, 56 dp cover at radius 12, prev / filled play 48 dp / next, and a 3 dp progress
  strip pinned to its bottom edge". It says nothing about vertical alignment.
- AGENTS.md's 800-line cap applies to every edited code file. `main/NowPlayingSheet.kt` (792 lines)
  and `main/BrowseScreen.kt` (792) sit at the cap: net growth there at most +5 lines; anything
  bigger goes into a cohesive sibling file. Never trim comments to fit.

## Tasks — test-first each: write the failing test, run it and see it fail, fix, see it pass, commit

### T1 — the mini player's content is vertically centred
`main/LibraryFrame.kt`, `MiniPlayer`: the `Row` inside the `Box` has no height and sits at the
Box's default `TopStart`, so the 56 dp cover touches the card's top edge and 16 dp stay empty below.
Fix: the `Row` fills the Box (`fillMaxSize()`). The cover then sits 8 dp from the card's top,
bottom and start edge — centred on the **whole 72 dp card** (grill decision). The 3 dp progress
strip stays where it is, pinned to the bottom edge and overlaying the card.
Test: render the bottom frame with a current track (follow the existing setup in tests that use
`miniPlayerHeightDp = 72`, e.g. `test/BrowseSurfaceTest.kt`), tag the mini player's cover
`library-mini-player-cover`, and assert via `getUnclippedBoundsInRoot()` that the cover's top inset
and bottom inset inside `library-mini-player` are both 8 dp (±0.5 dp). It must fail on the base.

### T2 — count labels use the singular for one
`main/LibraryText.kt` builds `"$trackCount tracks"` (`LibraryAlbum.details()`) and
`"$albumCount albums • $trackCount tracks"` (`LibraryArtist.details()`) unconditionally, so the
phone shows "1 albums • 1 tracks" and "Will Ramos • 2024 • 1 tracks". Fix: one `internal` helper in
`LibraryText.kt`, e.g. `countLabel(count: Long, singular: String, plural: String)`, used by both
`details()` functions → "1 album", "1 track", "0 tracks", "2 albums". Tests: unit cases for 0, 1, 2.

### T3 — the remaining-time label falls back to the track's length
`main/NowPlayingSheet.kt` (`SpectralSeekSlider`, ~line 633) takes `durationMs` from the player only.
A restored session sits paused and unprepared with player duration 0, so the label reads `--:--`
although the queue shows the track's length (e.g. 0:58). Fix **the label only** (grill decision):
add a pure function to `main/NowPlayingState.kt`, e.g.
`remainingLabel(positionMs, playerDurationMs, trackDurationMs)` = `formatRemaining(positionMs,
playerDurationMs.takeIf { it > 0 } ?: trackDurationMs)`, and pass the current track's `durationMs`
(`LibraryTrack.durationMs`) into the slider. The slider's `enabled`, its seek fraction and the
spectral bars stay on the player's duration — no seeking before the player is prepared. `--:--`
remains for "both unknown". `NowPlayingSheet.kt` net ≤ +5 lines.
Tests: pure-function cases (player known; player 0 + track known → `−0:58` at position 0;
both 0 → `--:--`), plus one Compose assertion that a paused session with player duration 0 shows
`−0:58` for a 58 s track while the slider stays disabled.

### T4 — artist detail page: one count line, album rows with covers
Today, top to bottom: summary row "5 albums · 0 other titles" (+ search, library ⋮) →
`ArtistDetailHeader` "← Will Ramos" (+ artist ⋮) → Play pill → portrait → "5 albums • 5 tracks"
(`ArtistPortraitHeader`, `main/ArtistCover.kt`) → "Albums" → `AlbumRow`s without a cover.
- **T4a** — `main/BrowseSummary.kt`: on an artist page the summary row reads
  `detail.artist.details()` ("5 albums • 5 tracks", with T2's singular handling). The
  "other titles" count leaves the summary row entirely; the list's own "Other titles" section
  heading stays as it is. The summary row shows exactly the string `details()` returns
  (separator ` • `, as everywhere else in the app) — `details()` is the single source, and the
  portrait line that used to show it is removed in T4b.
- **T4b** — `main/ArtistCover.kt` `ArtistPortraitHeader`: the count line under the portrait is
  removed. The portrait, its tags (`artist-portrait-head`, `artist-portrait-head-image`) and the
  shimmer backdrop stay.
- **T4c** — `main/BrowseTabs.kt` `AlbumRow` (~line 588): add the album cover as `leadingContent`
  from `album.representativeUri` — 56 dp, the same rounded-square shape as track-row covers,
  `AndroidArtworkSize.LIST`, no network fetch (`allowFetch = false`; reuse `TrackCover` or
  `rememberTrackArtworkVisual` + `ArtworkCover` from `main/TrackCover.kt`), decorative (the row's
  text already names the album). Applies to every `AlbumRow` use. Tag it `library-album-row-cover`.
- **Unchanged (grill decisions):** both overflow menus stay (the summary row's library menu is
  pinned on every destination by `MobileHeaderRowTest`; the header ⋮ is the artist's own actions);
  the search field keeps the summary row (match count + library menu) beneath it; the album detail
  page is not touched beyond `AlbumRow`.
Tests: in `test/ArtistDetailSurfaceTest.kt` / `test/ArtistPortraitSurfaceTest.kt` (or a new
sibling if a file would pass 800 lines): the summary row text on an artist page equals
`details()`; no node contains "other titles" in the summary row; the count text exists exactly once
on the page; each album row shows a `library-album-row-cover` node. Update existing assertions
that expected the count under the portrait.

Out of scope: the "N of M titles loaded" label (deliberate, `BrowseScreen.kt:417`), the "▷ 0"
play-count badges, any `[android]` UX rule.

## Verification for Codex
This plan touches ONLY `android/` and `docs/plans/`. The Rust gates in AGENTS.md do NOT apply —
do NOT run `cargo fmt/clippy/test/audit` yourself. One Gradle invocation at a time. Run, from the
worktree root, with this prefix (the sandbox needs worktree-local Gradle/Android homes):
```
export ANDROID_HOME=/home/marvin/.local/share/android-sdk ANDROID_SDK_ROOT=/home/marvin/.local/share/android-sdk \
  ANDROID_USER_HOME="$PWD/.cache/android-user-home" XDG_DATA_HOME="$PWD/.cache/xdg-data" \
  GRADLE_USER_HOME="$PWD/.gradle-user-home" JAVA_HOME=/usr/lib/jvm/java-21-openjdk TMPDIR=/tmp
printf 'sdk.dir=%s\n' "$ANDROID_HOME" > android/local.properties
scripts/check-android-suite.sh           # builds the FFI .so + Kotlin bindings itself, then the JVM suite
npm --prefix android run lint
scripts/check-android-theme.sh
```
While iterating, a filtered Gradle run (`android/gradlew --project-dir android :app:testDebugUnitTest
--tests '<Class>'`) is fine once the suite script has generated the bindings. Broad red across
unrelated suites: grep for "major version" (wrong JDK) before suspecting your change.

## Parallelität
Not cut (grill decision). T4 (`BrowseTabs.kt`, `ArtistCover.kt`, `BrowseSummary.kt`) and T1–T3
(`LibraryFrame.kt`, `LibraryText.kt`, `NowPlaying*.kt`) are disjoint, so a cut was possible, but the
expensive step is the Gradle/Robolectric suite per worktree and the diff is small: one strand, one
PR. No merge order, no post-merge cross-checks.

## After review (main session, not Codex)
Build the release APK from the worktree (`REPRISE_APK_WT=<worktree> ~/.cache/reprise-apk/build-apk.sh`),
check the signer matches the installed app (`apksigner verify --print-certs`), `adb install -r`
under `device-lock`, and take after-screenshots of the mini player, an artist page with a
one-track album, and Now Playing after a cold start — against the before-shots taken 2026-10-01.
