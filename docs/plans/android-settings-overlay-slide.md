---
slug: android-settings-overlay-slide
worktree: /home/marvin/Projects/reprise-android-settings-overlay-slide
branch: feature/android-settings-overlay-slide
phase: shipped
codex_session:
created: 2026-09-21
---
# The settings overlay slides like its pages

## Why

Entering and leaving the settings on Android are single-frame hard cuts,
measured on the Pixel 10 Pro XL on 2026-09-21 (handoff
`HANDOFF-2026-09-21-release-0.1.216-and-the-settings-transition.md`, evidence
strip `settings-overlay-cut-2026-09-21.png`): one frame in, one frame out,
against 36–39 frames for every page transition inside the settings graph.
#988 gave the graph its 300 ms slides and never touched the overlay's own
edge, so the seam is now louder than before — everything next to it moves.

The owner's report — "I see the old menu on the way out" — is this edge: from
a sub-page, back pops to the overview over 300 ms, the overview stands, and
then the whole overlay vanishes in one frame.

The seam is one `if`:

```kotlin
// BrowseScreen.kt:1048
if (settingsVisible) {
    Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
        when (val current = settingsState) {
            null -> { BackHandler { surfaceState.showSettings(false) }; PlaybackSettingsLoading(...) }
            else -> SettingsNavigation(state = current, ..., close = { surfaceState.showSettings(false) }, ...)
        }
    }
}
```

`settingsVisible` is a `mutableStateOf` on the activity-owned
`MobileSurfaceViewModel` (`MobileSurfaceViewModel.kt:131`), flipped by
`showSettings(Boolean)` (`:312`). It survives rotation; `settingsState` does not
(`BrowseScreen.kt:283-293`).

## What the fix is — and is not

**Deliverable:** the overlay's edge animates in both directions with the same
movement, duration and easing as the graph's pages. Entering = the graph's
`enterTransition`: slide in from the right over `SETTINGS_PAGE_SLIDE_MS` (300).
Leaving = the graph's `popExitTransition`: slide out to the right over the
same 300 ms. Default `tween` easing, exactly as `SettingsNavigation.kt:71-86`.
The browse screen underneath stays still and is revealed by the slide.

Grilled 2026-09-21; every point below was put to the owner and settled as
written here.

**Out of scope, deliberately:**

- *The coordinated two-edge fix* (intercepting the graph's pop mid-flight when
  the overlay closes). No measurement supports that case: run C showed the pop
  completes in full when the presses are ≥ 300 ms apart, and whether a real
  thumb ever gets under that is unmeasured. Not a requirement.
- *Parallax of the browse screen* (the graph moves the page behind by a quarter
  width). The browse content is not a page of the graph, it is the ground the
  overlay covers; giving it a `graphicsLayer` offset means wrapping ~800 lines
  of `BrowseScreen.kt` (already 1088 lines, over the 800 cap) in a new Box and
  sharing one `Transition` between the two. A modal cover sliding over a still
  ground is a complete pattern on its own. If the grill wants the parallax, it
  is a follow-up with its own measurement, not a rider on this one.
- *Any UX rule.* #988 — the directly analogous change, the inside of the same
  overlay — touched `SettingsNavigation.kt`, its test and `build.gradle.kts`
  and added no rule; section O (Motion) is `[gtk]` throughout and `[android]`
  appears zero times in `docs/ux-rules.md`. This plan follows that precedent.
  (The traceability gate does accept `[android]` since #788, so the reason is
  precedent and section scope, not gate blindness — the memory
  `an-android-scope-tag-would-hide-a-ux-rule` is stale on that mechanism.)

**Accepted consequence:** leaving from a sub-page with two quick back presses
now reads as pop (300 ms) → overlay slide (300 ms). Each press gets its own
300 ms movement; that is the graph's own rhythm, and it is what MOT-6's
principle ("the model changes at frame 0, the animation only illustrates")
asks for. Nothing is queued: `settingsVisible` flips on the press.

## Design

### The host: `settings/SettingsOverlay.kt` (new, ~50 lines)

```kotlin
/**
 * The opaque surface the settings graph is drawn inside, and the one edge the
 * graph does not own: entering and leaving the settings as a whole. It moves
 * exactly like a page of the graph — in from the right, out to the right, over
 * [SETTINGS_PAGE_SLIDE_MS] — so the overlay's edge and the pages inside it
 * read as one stack. Measured before this existed: both edges were one-frame
 * cuts next to 36-frame page slides.
 */
@Composable
internal fun SettingsOverlay(
    visible: Boolean,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    AnimatedVisibility(
        visible = visible,
        modifier = modifier,
        enter = slideInHorizontally(tween(SETTINGS_PAGE_SLIDE_MS)) { width -> width },
        exit = slideOutHorizontally(tween(SETTINGS_PAGE_SLIDE_MS)) { width -> width },
    ) {
        Surface(
            modifier = Modifier.fillMaxSize().testTag("settings-overlay"),
            color = MaterialTheme.colorScheme.background,
            content = content,
        )
    }
}
```

Two shape decisions, both pinned so Codex does not copy the sibling at
`BrowseScreen.kt:1002`:

1. **Plain `visible: Boolean`, not `visibleState = MutableTransitionState(false)`.**
   `AnimatedVisibility(visible = …)` seeds its transition *at* the initial
   value, so a composition that starts with `visible = true` shows the content
   at rest without animating. That is exactly the rotation case:
   `settingsVisible` comes back `true` from the ViewModel and the overlay must
   simply be there (`ComposeBehaviorTest.settingsSurviveARotationEvenWhenTheReloadFails`).
   The now-playing sheet's `remember { MutableTransitionState(false) }` shape
   would slide the settings in after every rotation.
2. **The `testTag` sits on the inner `Surface`, not on `AnimatedVisibility`.**
   The slide is applied to the animated child's layout; a tag on the outer
   node reports the host's bounds, which never move.

`SETTINGS_PAGE_SLIDE_MS` is `internal` in the same module; no change to
`SettingsNavigation.kt`'s constants. The lambdas are the graph's own
(`{ width -> width }`), copied, not re-derived.

### The seam in `BrowseScreen.kt`

`if (settingsVisible) { Surface(...) { when ... } }` becomes
`SettingsOverlay(visible = settingsVisible) { when ... }`. The `when` and
everything inside it move one level, unchanged. Nothing else in the file
moves; this is a ~10-line edit, not a "substantial" edit in the sense of the
800-line rule, and the file does not get split in this plan.

### The back button during the exit window

During the 300 ms exit `settingsVisible` is already `false`, so
`BrowseScreen.kt:700`'s handler (enabled by `!settingsVisible && …`) may be
enabled — while the departing overlay is still composed with
`SettingsNavigation.kt:58`'s `BackHandler(enabled = route == null || route == OVERVIEW)`
still enabled. Compose registers callbacks in composition order and the
dispatcher gives the press to the most recently registered enabled callback:
the departing settings win, `close()` runs `showSettings(false)` again, and the
press is swallowed. A 300 ms window in which back does nothing, on the surface
that just said "I am leaving".

Fix: the overlay's handlers are gated on the overlay being open, not only on
the route. `SettingsNavigation` gets `active: Boolean = true` (default keeps
`SettingsPageTransitionTest` and `SettingsContentTest` untouched) and its
handler becomes `BackHandler(enabled = active && (route == null || route == OVERVIEW))`.
The loading branch's `BackHandler { showSettings(false) }` in `BrowseScreen.kt`
becomes `BackHandler(enabled = settingsVisible) { … }`. `BrowseScreen` passes
`active = settingsVisible`.

The `NavHost`'s own back handler (enabled while a sub-page is on the stack) is
left alone: the only way to leave from a sub-page is `LibrarySettingsPage`'s
chooseFolder/rescan path, where the screen underneath is being replaced anyway,
and a pop inside a departing overlay is harmless.

## Tasks — test first, in this order

### Task 1 — `SettingsOverlayTransitionTest.kt` (new, `android/app/src/test/…/reprise/`)

Robolectric, `@GraphicsMode(NATIVE)`, `@Config(sdk = [36], qualifiers = "w412dp-h916dp-port")`,
`createAndroidComposeRule<ComponentActivity>()`, mounted inside `RepriseTheme`
like `SettingsPageTransitionTest`. It mounts `SettingsOverlay(visible = state)`
around a `Box(Modifier.fillMaxSize().testTag("payload"))` with `state` a
`mutableStateOf` the test flips. Mid-flight measurement copies
`SettingsPageTransitionTest.captureMidTransition`: `mainClock.autoAdvance = false`,
flip, `advanceTimeByFrame()`, `advanceTimeBy(SETTINGS_PAGE_SLIDE_MS / 2L)`,
read `onNodeWithTag("settings-overlay").getUnclippedBoundsInRoot().left` in px.

- `theOverlayArrivesFromTheRightOverThePageSlideDuration` — after the flip to
  `true` and half the duration: the overlay exists and `0 < left < hostWidth`.
  After `autoAdvance = true` + `waitForIdle()`: `left == 0`.
- `theOverlayLeavesToTheRightAndIsGoneAfterwards` — start visible (at rest),
  flip to `false`, half duration: the overlay still exists and `left > 0`;
  after `waitForIdle()`: `onAllNodesWithTag("settings-overlay").assertCountEquals(0)`.
- `anOverlayRestoredOpenStandsStillFromTheFirstFrame` — first composition with
  `visible = true`, `autoAdvance = false`, one frame: `left == 0` and the node
  exists. This is the rotation guard; it turns red with the
  `MutableTransitionState(false)` shape.

Run it, see it fail (`Unresolved reference: SettingsOverlay`), then Task 2.

### Task 2 — `SettingsOverlay.kt` + the seam

Write `settings/SettingsOverlay.kt` as designed. Replace the `if` block in
`BrowseScreen.kt` with `SettingsOverlay(visible = settingsVisible) { … }`.
Task 1 green. `MainActivitySettingsNavigationTest`,
`SettingsPageTransitionTest`, `SettingsContentTest` and
`ComposeBehaviorTest` stay green untouched — their `waitForIdle()` calls run
the clock through the new 300 ms, and `assertCountEquals(0)` after a close
still holds once the exit has finished.

### Task 3 — the exit window gives the back button back

Test first, in `MainActivitySettingsNavigationTest.kt` (it already owns the
activity fixture and `openSettings()`):

- `aBackPressDuringTheClosingSlideReachesWhatIsUnderneath` — register a test
  `OnBackPressedCallback` on `compose.activity.onBackPressedDispatcher`
  **before** `openSettings()` (added earlier = lower priority, so the settings
  win while open). Open settings, `waitForIdle()`. `mainClock.autoAdvance = false`.
  `onBackPressed()` once → `settingsVisible` false, overlay still composed
  (`onNodeWithTag("settings-overlay")` exists after `advanceTimeByFrame()`).
  `onBackPressed()` again → the test callback fired exactly once. Before Task 3's
  code, the second press is swallowed by the departing settings and the
  callback count stays 0.

Then the code: `active` on `SettingsNavigation`, the gated loading-branch
handler, `active = settingsVisible` at the call site.

### Task 4 — suite and lint

`scripts/check-android-suite.sh` green (all suites executed, the two new/changed
test classes among them), `npm --prefix android run lint` green.

## Rules for the coding run

- **This plan touches ONLY `android/` and `docs/`. The Rust gates in AGENTS.md
  do not apply — do NOT run any cargo command yourself.** The one cargo build
  the Android suite needs is done by `scripts/check-android-suite.sh` itself.
  One Gradle invocation at a time.
- The suite is `scripts/check-android-suite.sh`, never raw `gradlew` — the
  JVM tests need the host `.so` and bindings it generates. Run it with
  `GRADLE_OPTS='-Dorg.gradle.daemon=false'` (another Android worktree is live
  on this machine; a shared daemon loads the other worktree's library).
- A fresh worktree has no `android/local.properties` (copy it from the main
  checkout: `sdk.dir=/home/marvin/.local/share/android-sdk`) and no
  `android/app/src/main/java/uniffi/` — generate it (the suite script does),
  never copy it.
- Do **not** touch `versionCode`/`versionName` in `android/app/build.gradle.kts`;
  `land.sh` bumps them.
- English everywhere; test names camelCase like their neighbours (no rule ID
  to carry).

## Verification on the device (after landing — not Codex's)

Under `device-lock acquire`, same instrument as the 2026-09-21 measurement:
`adb shell screenrecord` (frames are written only on change; 120 Hz → ~8.3 ms
per frame), the same path overview → Audio → back → back, presses spaced
≥ 400 ms apart with the achieved gap read back from the recording. Expected:
entering settings and leaving settings each span ~36 frames / ~300 ms, in the
same band as the page push/pop; no one-frame span remains on either edge.
Also: rotate with the settings open — the overlay is at rest in the first frame
after the rotation, not sliding in.

## Parallelität

Not cut. One new file (`SettingsOverlay.kt`), one edit at a single seam in
`BrowseScreen.kt`, one parameter in `SettingsNavigation.kt`, and two test files
that mount those same composables — every task reads or writes the same three
production files, and Task 3's test only makes sense against Task 2's overlay.
No disjoint file group exists. Single strand, no suffix files.
