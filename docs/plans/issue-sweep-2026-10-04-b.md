---
slug: issue-sweep-2026-10-04-b
worktree: /home/marvin/Projects/reprise-issue-sweep-2026-10-04-b
branch: feature/issue-sweep-2026-10-04-b
phase: planned
codex_session:
created: 2026-10-04
---
# Strand b — the mini-player grows with the font scale (#1063), and the write-lane test stops racing (#1058)

## #1063 diagnosis
    #1063 diagnosis (worker, origin/dev 78005329e3):
    Root cause: LibraryFrame.kt MiniPlayer (:272): Surface `.height(metrics.miniPlayerHeightDp.dp)` (:286) = constant 72
    (LibraryFramePolicy.kt:18 STACKED, :26 WIDE_SHORT), clip to shapes.large (:293); inner Row fillMaxSize() with 8 dp
    horizontal padding only (:297-299); text Column (:312-325) titleMedium 24sp + bodyMedium 20sp exceeds 72 dp at 2.0.
    Dependants: none hold a constant. BrowseScreen.kt:564 puts LibraryBottomFrame in Scaffold(bottomBar) and lists use the
    Scaffold contentPadding (measured). Hide animation (LibraryFrame.kt:168-176) uses measured size.height. Dock/WIDE_SHORT
    use the same MiniPlayer.
    Fix (#1064 pattern): Surface `.heightIn(min = minimum, max = if (fontScale <= 1f) minimum else Dp.Unspecified)`, keep
    clip. CATCH: the Row's fillMaxSize() must go (bottomBar slot is bounded, so it would fill the screen). Use
    fillMaxWidth().heightIn(min = minimum).padding(horizontal = 8.dp, vertical = 8.dp); cover 56 dp keeps 72 dp floor and
    8 dp inset at 1.0; progress rail align(BottomStart) stays. The constant becomes a floor, so comment or rename it.
    Tests: new MiniPlayerFontScaleTest.kt (@Config sdk 36, w500dp-h1000dp) modelled on LibraryTrackRowFontScaleTest:
    linear 2.0 text inside library-mini-player, no didOverflowHeight, 8 dp above the title and below the subtitle;
    nonlinear-density 2.0 card exceeds the typography estimate by >= 8 dp; exactly 72 dp at 1.0 (STACKED and WIDE_SHORT);
    at 2.0 the last list row bottom <= mini-player top. Each must fail on the base. Existing 72 dp tests
    (MainActivityConfigurationTest.kt:434, MiniPlayerLayoutTest.kt:41-42) stay. No UX rule exists for this.
    Device check at 2.0: descenders, last row clears the mini-player, now-playing open/close anim, 1.0 unchanged,
    landscape/WIDE_SHORT. Also look at the NavigationBar (fixed 80 dp, ~:203) labels at 2.0.

## #1058 diagnosis
    #1058 diagnosis (worker, read on origin/dev 78005329e3): TEST is wrong, production correct.
    LibraryWritesTest.aFailingWriteIsReportedAndTheLaneKeepsRunning (lines 145-168) uses onMainThread = inline and
    drainTimeoutMs = 0; report = answers::put wakes the test thread inside report, before returnPending() in the finally
    (LibraryWrites.kt:102-108), so shutdown() (141-157) sees answeredPending=1 and withTimeoutOrNull(0) returns false.
    Production hops report to the UI thread (MainActivity.kt:114-116) and shutdown runs in onDestroy on the same thread.
    Fix (test only): before shutdown(), submit a sentinel submitUnanswered(work = idle::countDown) and await it (lane is
    limitedParallelism(1)), the pattern used by answeredPendingReturnsToZeroAfterTheReportIsHandedOver (254-261).
    Add shutdownReportsNotDrainedWhileAnAnswerIsStillBeingDelivered: hold report on a latch, assert shutdown()==false,
    release, sentinel, assert shutdown()==true — pins the ordering so nobody "fixes" production. Only LibraryWritesTest.kt.

## Tasks (test-first)
1. #1063: the mini-player Surface takes `heightIn(min = 72 dp, max = 72 dp only at font scale <= 1.0)`, its inner Row
   drops `fillMaxSize()` for `fillMaxWidth().heightIn(min).padding(horizontal = 8.dp, vertical = 8.dp)`, the clip
   stays. The policy constant is documented as a floor. New `MiniPlayerFontScaleTest.kt` with the tests listed above
   (linear 2.0, nonlinear 2.0 that fails on a typography estimate, exact 72 dp at 1.0 for STACKED and WIDE_SHORT, list
   content clears the mini-player at 2.0). Each must fail on the base commit.
2. #1058: test-only fix in `LibraryWritesTest.kt` (sentinel before `shutdown()`), plus
   `shutdownReportsNotDrainedWhileAnAnswerIsStillBeingDelivered` pinning the ordering. No production change.

## Verification
Android suite, Android lint, theme lint. Device check at font scale 2.0 (Opus, under device-lock): mini-player
descenders, last list row clears it, now-playing open/close animation, 1.0 unchanged, landscape, nav bar labels.
