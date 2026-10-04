---
slug: issue-sweep-2026-10-03-b
worktree: /home/marvin/Projects/reprise-issue-sweep-2026-10-03-b
branch: feature/issue-sweep-2026-10-03-b
phase: shipped
codex_session:
created: 2026-10-03
---
# Strand b — a flaky test waits for its node, and track rows grow with the font scale (#1000, #1055)

Mother plan: `docs/plans/issue-sweep-2026-10-03.md`. Paths below are under
`android/app/src/{main,test}/java/io/github/marvinbaudach/reprise/`.

## #1000 — flaky `MainActivityMusicPathsTest`

`MainActivityMusicPathsTest.kt:147` asserts "Back to artists" is displayed right after
`recreate()` / `waitForIdle()`, without the `waitUntil` guard that the same node gets at lines
138–139 before the recreate. On a slow CI runner the node is not composed yet.

- Add the same `compose.waitUntil(timeoutMillis = 5_000) { …fetchSemanticsNodes().isNotEmpty() }`
  guard before the assertion at 147. Check the rest of the file for other post-`recreate()`
  assertions without a guard and give them the same treatment.
- No production change. The fix commit body says `Closes #1000` is premature (it can only be
  proven over CI runs); write `Refs #1000` instead.

## #1055 — track rows clip at font scale 2.0

- `LibraryTrackRows.kt:352-353` sets `.height(metrics.trackRowHeightDp.dp)` and
  `.clipToBounds()`. `LibraryFramePolicy.kt` holds the value as a fixed 72 dp (STACKED) or
  64 dp (WIDE_SHORT), with no font scale. The text inside scales, so at 2.0 the subtitle and
  duration are clipped.
- The same value feeds `queueDragMotion(rowHeightDp = …)` (line 345) and `QueueDragHandle`
  (line 433, converted to px and keyed into `pointerInput`, feeding `QueueReorder.kt`'s drop
  target). Scroll restore in `LibraryListAnchor.kt` measures rows at runtime and needs no change.

**Required behaviour.** At font scale ≤ 1.0 the row heights stay exactly 72 / 64 dp. Above
1.0 the row grows so that the title line, the subtitle line and the trailing duration are fully
inside the row. Nothing is dropped. Every consumer — the row's own height, `queueDragMotion` and
`QueueDragHandle` — reads the one effective height, so a queue drag at 2.0 still moves rows by
exactly one row.

**Shape.** One function computes the effective row height from the layout's base height and the
font scale (for example: base height, or the scaled text block plus the row's vertical padding,
whichever is larger). Derive the text block from the typography the row actually uses, not
from a guessed constant. `LibraryTrackRow` reads `LocalDensity.current.fontScale` once and
passes the result to all three consumers. Keep `.clipToBounds()`.

### Tasks (test-first)

1. Failing Robolectric Compose test at font scale 2.0 (see `PlayCountBadgeTest.kt` for the
   existing font-scale setup): a track row's title, subtitle and duration nodes lie fully inside
   the row's bounds. Name it after the issue's symptom.
2. A test that the effective height at font scale 1.0 is still 72 and 64 dp.
3. A test that the drag handle and the drag motion get the same effective height as the row at
   2.0 (through the real composable, not by calling the helper twice).
4. Implement. Update tests that hard-code 72/64 only where they run at a scale above 1.0 or
   where the value they assert genuinely changed; otherwise leave them.
5. Gates, with the worktree-local environment prefix
   (`ANDROID_HOME`/`ANDROID_SDK_ROOT=/home/marvin/.local/share/android-sdk`,
   `ANDROID_USER_HOME="$PWD/.cache/android-user-home"`, `XDG_DATA_HOME="$PWD/.cache/xdg-data"`,
   `GRADLE_USER_HOME="$PWD/.gradle-user-home"`): `scripts/check-android-suite.sh`,
   `npm --prefix android run lint`, `scripts/check-android-theme.sh`. `LibraryTrackRows.kt` is at
   706 lines; it stays under 800, or the height logic moves into `LibraryFramePolicy.kt`.

The #1055 fix commit body says `Closes #1055`. The device check at font scale 2.0 is the
orchestrator's, after this strand.
