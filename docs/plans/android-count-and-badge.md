---
slug: android-count-and-badge
worktree: /home/marvin/Projects/reprise-android-count-and-badge
branch: feature/android-count-and-badge
phase: coded
codex_session:
created: 2026-10-02
---
# A track never played carries no play-count badge on the phone

Source: `docs/plans/HANDOFF-2026-10-02-android-ux-open-findings.md`, finding F-B. The decision
was put to the user on 2026-10-02 and answered with the recommendation.

F-A (the "200 of 727 titles loaded" label) is **out of scope**. A parallel session implements it
on `fix/android-count-names-the-library`, and this branch touches none of its files.

**Android-only.** No Rust crate, no `cargo`, no `docs/ux-rules.md` change (no `[android]` UX
rule — precedent #988). All paths below are under
`android/app/src/{main,test}/java/io/github/marvinbaudach/reprise/`.

## Task — F-B: no play-count badge for a track never played

`LibraryTrackRows.kt` ~443 draws `PlayCountBadge(track.playCount)` in the trailing 48 dp column
for every non-current row, queue included; `PlayCountBadge` (~695) always draws, even for 0.

Target: when `playCount <= 0` no badge is drawn and no play-count description is announced.
The row's merged content description simply has no play count, and nothing says "0 plays".

- **Alignment:** the trailing `Column` is centred vertically in its row. Removing the badge
  would centre the duration on its own, so durations would sit at a different height in played
  and unplayed rows. Reserve the slot instead. Render the real badge invisibly
  (`Modifier.alpha(0f)` plus `clearAndSetSemantics {}`), not a box with a hardcoded dp height:
  the badge's height comes from `labelSmall` and a 12 sp symbol, so a fixed height drifts under
  font scaling. The invisible badge matches at every scale and announces nothing. (Settled in the grill
  2026-10-02: reserve the slot.)
- Keep the `PlayingBars` branch for the current row unchanged.

Tests:
- New Compose test (next to `ComposeBehaviorTest.kt`'s play-count assertions ~205–217): a row
  with `playCount = 0` has no badge text ("0") in the semantics tree, and its merged content description has no "play" in
  it; a row with `playCount = 1` still announces `"1 play"`. Write it first and see it fail.
- Assert that the duration text's top is equal for a 0-play row and a
  27-play row.
- `MainActivityDockTest.kt:70` and `MainActivityRatingTest.kt:166` already assert that
  `"0 plays"` text is absent. They must stay green; leave them alone.

## Gates (Android-only, no cargo)

Environment prefix from the handoff (`ANDROID_HOME`, `ANDROID_SDK_ROOT`, `ANDROID_USER_HOME`,
`XDG_DATA_HOME`, `GRADLE_USER_HOME`, `JAVA_HOME`, `TMPDIR`, `android/local.properties`). The
Android suite script is the verdict, because filtered gradle runs hit `UnsatisfiedLinkError`.
Then:
- `npm --prefix android run lint`;
- theme, accessibility-semantics, input-parity, ux-traceability, shared-literals,
  duration-format-parity, ai-hygiene and architecture check scripts.

## Device check after landing

Build from dev with `~/.cache/reprise-apk/build-apk.sh`, run `apksigner verify --print-certs`,
then `adb install -r` under `device-lock`. Then:
- the Queue rows of never-played tracks have no "▷ 0" and their durations line up.

## Parallelität

No cut. One task in one source file (`LibraryTrackRows.kt`) plus its test. Disjoint from the
parallel F-A branch, whose files are `LibraryScreenState.kt`, `BrowseSummary.kt`,
`BrowseScreen.kt`, `NowPlayingQueue.kt` and their tests. Either branch can land first without
conflict.
