package de.reprise.spike

import android.graphics.Bitmap
import android.graphics.Canvas as AndroidCanvas
import android.view.ViewGroup
import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.compositeOver
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModelStore
import de.reprise.spike.settings.OnlineSourcesSettingsPage
import de.reprise.spike.ui.theme.RepriseTheme
import java.util.concurrent.ConcurrentLinkedQueue
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.reprise_android_ffi.AndroidColorScheme
import kotlin.math.roundToInt

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h1000dp")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ArtistPhotoProgressBarTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun preparingHasNoCounterAndUsesIndeterminateProgress() {
        show(ArtistPhotoProgress(1, ArtistPhotoProgressPhase.PREPARING, 0, 0, 0))

        compose.onNodeWithText("Preparing artist photos").assertIsDisplayed()
        compose.onNodeWithTag("artist-photo-progress-counter").assertDoesNotExist()
        compose.onNodeWithTag("artist-photo-progress-track").assertIsDisplayed()
    }

    @Test
    fun runningShowsTheDownloadedCountAndDeterminateProgress() {
        show(ArtistPhotoProgress(2, ArtistPhotoProgressPhase.RUNNING, 128, 0, 412))

        compose.onNodeWithText("Downloading artist photos").assertIsDisplayed()
        compose.onNodeWithText("128 / 412").assertIsDisplayed()
        compose.onNodeWithTag("artist-photo-progress-track")
            .assertProgress(128f / 412f)
    }

    @Test
    fun determinateProgressHasOneCountAnnouncement() {
        show(ArtistPhotoProgress(2, ArtistPhotoProgressPhase.RUNNING, 128, 0, 412))

        compose.onNodeWithTag("artist-photo-progress-track")
            .assert(
                SemanticsMatcher.expectValue(
                    SemanticsProperties.StateDescription,
                    "Artist photos, 128 of 412 downloaded",
                ),
            )
            .assert(SemanticsMatcher.keyNotDefined(SemanticsProperties.ContentDescription))
    }

    @Test
    fun successfulCompletionIsAllTealAndHasNoFailureLine() {
        show(ArtistPhotoProgress(3, ArtistPhotoProgressPhase.COMPLETE, 412, 0, 412))

        compose.onNodeWithText("Artist photos complete").assertIsDisplayed()
        compose.onNodeWithText("412 / 412").assertIsDisplayed()
        compose.onNodeWithTag("artist-photo-progress-track").assertProgress(1f)
        compose.onNodeWithTag("artist-photo-progress-failure").assertDoesNotExist()
    }

    @Test
    fun failedCompletionUsesTheDesignCounts() {
        show(ArtistPhotoProgress(4, ArtistPhotoProgressPhase.COMPLETE, 397, 15, 412))

        compose.onNodeWithText("397 / 412").assertIsDisplayed()
        compose.onNodeWithText("15 without a photo").assertIsDisplayed()
        compose.onNodeWithTag("artist-photo-progress-track").assertProgress(1f)
    }

    @Test
    fun failedCompletionRendersTealThenPurpleAtTheMeasuredSplit() {
        show(ArtistPhotoProgress(4, ArtistPhotoProgressPhase.COMPLETE, 397, 15, 412))

        val track = compose.onNodeWithTag("artist-photo-progress-track")
            .getUnclippedBoundsInRoot()
        val pixels = renderActivity()
        val density = compose.activity.resources.displayMetrics.density
        val middleY = (
            (track.top.value + (track.bottom.value - track.top.value) / 2f) * density
        ).roundToInt()
        val left = (track.left.value * density).roundToInt()
        val width = ((track.right.value - track.left.value) * density).roundToInt()
        val teal = pixels[left + width / 2, middleY]
        assertTrue(
            "transparent sample: track=$track density=$density bitmap=${pixels.width}x${pixels.height} " +
                "sample=${left + width / 2},$middleY",
            teal.alpha > 0.9f,
        )

        assertColorNear(Color(0xFF4FDBD4), teal)
        assertColorNear(Color(0xFF9184D9), pixels[left + (width * 98) / 100, middleY])
    }

    @Test
    fun pausedKeepsItsLastDeterminateStand() {
        show(ArtistPhotoProgress(5, ArtistPhotoProgressPhase.PAUSED, 128, 15, 412))

        compose.onNodeWithText("Waiting for a connection").assertIsDisplayed()
        compose.onNodeWithText("128 / 412").assertIsDisplayed()
        compose.onNodeWithTag("artist-photo-progress-track")
            .assertProgress(143f / 412f)
    }

    @Test
    fun noRunCreatesNoNode() {
        show(null)

        compose.onNodeWithTag("artist-photo-progress").assertDoesNotExist()
    }

    @Test
    fun noRunLeavesNoNodeAndNoLayoutSpace() {
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                Column {
                    Spacer(Modifier.fillMaxWidth().height(7.dp).testTag("before-progress"))
                    ArtistPhotoProgressBar(progress = null, dismiss = {})
                    Spacer(Modifier.fillMaxWidth().height(7.dp).testTag("after-progress"))
                }
            }
        }

        compose.onAllNodesWithTag("artist-photo-progress", useUnmergedTree = true)
            .assertCountEquals(0)
        val before = compose.onNodeWithTag("before-progress").getUnclippedBoundsInRoot()
        val after = compose.onNodeWithTag("after-progress").getUnclippedBoundsInRoot()
        assertEquals(before.bottom, after.top)
    }

    @Test
    fun dismissalSticksForOneRunAndClearsForTheNext() {
        val viewModel = MobileSurfaceViewModel()
        viewModel.acceptArtistPhotoProgress(running(runId = 9))

        viewModel.dismissArtistPhotoProgress()
        viewModel.acceptArtistPhotoProgress(running(runId = 9, done = 2))
        assertEquals(null, viewModel.visibleArtistPhotoProgress)

        viewModel.acceptArtistPhotoProgress(running(runId = 10))
        assertEquals(10L, viewModel.visibleArtistPhotoProgress?.runId)
    }

    @Test
    fun composableStaysHiddenForTheDismissedRunAndReturnsForTheNextRun() {
        val viewModel = MobileSurfaceViewModel()
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                ArtistPhotoProgressBar(
                    progress = viewModel.visibleArtistPhotoProgress,
                    dismiss = viewModel::dismissArtistPhotoProgress,
                )
            }
        }
        compose.runOnIdle { viewModel.acceptArtistPhotoProgress(running(runId = 30)) }
        compose.onNodeWithTag("artist-photo-progress").assertIsDisplayed()

        compose.runOnIdle { viewModel.dismissArtistPhotoProgress() }
        compose.onNodeWithTag("artist-photo-progress").assertDoesNotExist()
        compose.runOnIdle {
            viewModel.acceptArtistPhotoProgress(running(runId = 30, done = 2))
        }
        compose.onNodeWithTag("artist-photo-progress").assertDoesNotExist()

        compose.runOnIdle { viewModel.acceptArtistPhotoProgress(running(runId = 31)) }
        compose.onNodeWithTag("artist-photo-progress").assertIsDisplayed()
    }

    @Test
    fun animatedSegmentFractionsCannotInvert() {
        val completed = 0.8f
        val done = clampedArtistPhotoDoneFraction(
            animatedDone = 0.9f,
            animatedCompleted = completed,
        )

        assertEquals(0.8f, done)
        assertEquals(0f, completed - done)
    }

    @Test
    fun mutedTextAlphaCompositesNearTheDesignColourOnTheCard() {
        val composite = Color(0xFFB2B6CA)
            .copy(alpha = ARTIST_PHOTO_MUTED_ALPHA)
            .compositeOver(Color(0xFF292B31))
        val target = Color(0xFF8F96A3)

        assertTrue(kotlin.math.abs(composite.red - target.red) <= 2f / 255f)
        assertTrue(kotlin.math.abs(composite.green - target.green) <= 2f / 255f)
        assertTrue(kotlin.math.abs(composite.blue - target.blue) <= 2f / 255f)
    }

    @Test
    fun aBackgroundSnapshotIsPostedBeforeItMutatesComposeState() {
        val posted = ConcurrentLinkedQueue<() -> Unit>()
        var snapshot = running(runId = 20)
        val viewModel = MobileSurfaceViewModel()
        viewModel.bindArtistPhotoBackfill(
            snapshot = { snapshot },
            start = {},
            cancel = {},
            postToMain = posted::add,
        )
        posted.remove().invoke()
        assertEquals(20L, viewModel.visibleArtistPhotoProgress?.runId)
        snapshot = running(runId = 21)

        Thread(viewModel::startArtistPhotoBackfill).also {
            it.start()
            it.join()
        }

        assertEquals(20L, viewModel.visibleArtistPhotoProgress?.runId)
        assertEquals(1, posted.size)
        posted.remove().invoke()
        assertEquals(21L, viewModel.visibleArtistPhotoProgress?.runId)
    }

    @Test
    fun lateBackfillCommandsAfterClearDoNotReachTheClosedLibraryBinding() {
        var starts = 0
        var cancels = 0
        val viewModel = MobileSurfaceViewModel()
        viewModel.bindArtistPhotoBackfill(
            snapshot = { running(runId = 22) },
            start = { starts += 1 },
            cancel = { cancels += 1 },
        )
        ViewModelStore().apply {
            put("surface", viewModel)
            clear()
        }
        assertEquals(1, cancels)

        viewModel.startArtistPhotoBackfill()
        viewModel.cancelArtistPhotoBackfill()

        assertEquals(0, starts)
        assertEquals(1, cancels)
    }

    @Test
    fun dismissButtonReportsHideProgress() {
        var dismissals = 0
        show(running(runId = 7), dismiss = { dismissals += 1 })

        compose.onNodeWithContentDescription("Hide progress").performClick()

        assertEquals(1, dismissals)
    }

    @Test
    fun successfulCompletionDismissesAfterFourSeconds() {
        var dismissals = 0
        compose.mainClock.autoAdvance = false
        show(
            ArtistPhotoProgress(12, ArtistPhotoProgressPhase.COMPLETE, 412, 0, 412),
            dismiss = { dismissals += 1 },
        )

        compose.mainClock.advanceTimeBy(4_001)
        compose.waitForIdle()

        assertEquals(1, dismissals)
    }

    @Test
    fun onlineSourcesUsesTheSameProgressLabels() {
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                OnlineSourcesSettingsPage(
                    enabled = true,
                    setEnabled = {},
                    progress = running(runId = 8),
                    dismissProgress = {},
                    back = {},
                )
            }
        }

        compose.onNodeWithText("Downloading artist photos").assertIsDisplayed()
        compose.onNodeWithText("1 / 412").assertIsDisplayed()
    }

    @Test
    fun browseUsesTheSameProgressLabelsAboveItsPager() {
        val viewModel = MobileSurfaceViewModel().apply {
            acceptArtistPhotoProgress(running(runId = 11))
        }
        val browse = LibraryScreenState.Browse(
            titles = LibraryWindow.empty(),
            artists = LibraryWindow.empty(),
        )
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                BrowseScreen(
                    state = browse,
                    playback = PlaybackUiState().libraryPlayback(),
                    playbackSettingsRevision = 0,
                    surfaceState = viewModel,
                    chooseFolder = {},
                    rescan = {},
                    searchTitles = { _, _ -> LibraryWindow.empty() },
                    listArtists = { LibraryWindow.empty() },
                    openAlbum = { error("Album navigation is outside this test") },
                    listAlbumTracks = { _, _ -> LibraryWindow.empty() },
                    loadTrack = { _, deliver -> deliver(null) },
                    playTracks = { _, _ -> },
                    loadPlaybackSettings = { PlaybackSettingsUiState(false, true, emptyList()) },
                    setEqualizerEnabled = { PlaybackSettingsUiState(it, true, emptyList()) },
                    replaceEqualizerCurve = {
                        PlaybackSettingsUiState(false, true, emptyList())
                    },
                    setGaplessEnabled = { PlaybackSettingsUiState(false, it, emptyList()) },
                    themeSelection = theme,
                    selectTheme = {},
                )
            }
        }

        compose.onNodeWithText("Downloading artist photos").assertIsDisplayed()
        compose.onNodeWithText("1 / 412").assertIsDisplayed()
        compose.onNodeWithTag("library-destination-pager").assertIsDisplayed()
    }

    @Test
    fun net_4b_browseShowsTheArtistPhotoOfferAndNotNowRemovesIt() {
        val settled = mutableStateOf(false)
        showBrowseWithArtistOffer(settled) { settled.value = true }

        compose.onNodeWithText("Show artist photos?").assertIsDisplayed()
        compose.onNodeWithText("Not now").performClick()

        compose.onNodeWithText("Show artist photos?").assertDoesNotExist()
        compose.onNodeWithTag("library-destination-pager").assertIsDisplayed()
    }

    @Test
    fun net_4b_downloadingArtistPhotosRemovesTheOfferBeforeProgressCanReplaceIt() {
        val settled = mutableStateOf(false)
        var downloads = 0
        showBrowseWithArtistOffer(
            settled = settled,
            download = {
                settled.value = true
                downloads += 1
            },
        )

        compose.onNodeWithText("Download artist photos").performClick()

        assertEquals(1, downloads)
        compose.onNodeWithText("Show artist photos?").assertDoesNotExist()
    }

    private fun show(
        initial: ArtistPhotoProgress?,
        dismiss: () -> Unit = {},
    ) {
        val state = mutableStateOf(initial)
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                ArtistPhotoProgressBar(progress = state.value, dismiss = dismiss)
            }
        }
    }

    private fun showBrowseWithArtistOffer(
        settled: androidx.compose.runtime.MutableState<Boolean>,
        notNow: () -> Unit = { settled.value = true },
        download: () -> Unit = { settled.value = true },
    ) {
        val artist = LibraryArtist("Slowdive", 12, 4, "content://slowdive")
        val browse = LibraryScreenState.Browse(
            titles = LibraryWindow.empty(),
            artists = LibraryWindow(total = 1, rows = listOf(artist), hasMore = false),
        )
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                BrowseScreen(
                    state = browse,
                    playback = PlaybackUiState().libraryPlayback(),
                    playbackSettingsRevision = 0,
                    surfaceState = MobileSurfaceViewModel(),
                    chooseFolder = {},
                    rescan = {},
                    searchTitles = { _, _ -> LibraryWindow.empty() },
                    listArtists = { browse.artists },
                    openAlbum = { error("Album navigation is outside this test") },
                    listAlbumTracks = { _, _ -> LibraryWindow.empty() },
                    loadTrack = { _, deliver -> deliver(null) },
                    playTracks = { _, _ -> },
                    loadPlaybackSettings = { PlaybackSettingsUiState(false, true, emptyList()) },
                    setEqualizerEnabled = { PlaybackSettingsUiState(it, true, emptyList()) },
                    replaceEqualizerCurve = {
                        PlaybackSettingsUiState(false, true, emptyList())
                    },
                    setGaplessEnabled = { PlaybackSettingsUiState(false, it, emptyList()) },
                    themeSelection = theme,
                    selectTheme = {},
                    artistPhotoOfferSettled = settled.value,
                    downloadArtistPhotos = download,
                    declineArtistPhotos = notNow,
                )
            }
        }
    }

    private fun running(runId: Long, done: Long = 1) = ArtistPhotoProgress(
        runId = runId,
        phase = ArtistPhotoProgressPhase.RUNNING,
        done = done,
        failed = 0,
        total = 412,
    )

    private fun assertColorNear(expected: Color, actual: Color) {
        assertTrue("red: expected $expected, got $actual", kotlin.math.abs(expected.red - actual.red) < 0.02f)
        assertTrue(
            "green: expected $expected, got $actual",
            kotlin.math.abs(expected.green - actual.green) < 0.02f,
        )
        assertTrue("blue: expected $expected, got $actual", kotlin.math.abs(expected.blue - actual.blue) < 0.02f)
    }

    private fun renderActivity(): androidx.compose.ui.graphics.PixelMap {
        val content = compose.activity.findViewById<ViewGroup>(android.R.id.content)
        val bitmap = Bitmap.createBitmap(content.width, content.height, Bitmap.Config.ARGB_8888)
        content.draw(AndroidCanvas(bitmap))
        return bitmap.asImageBitmap().toPixelMap()
    }

    private val theme = MobileThemeSelection(
        palette = MobileTheme.NOCTURNE,
        colorScheme = AndroidColorScheme.SYSTEM,
        dynamicAvailable = false,
    )
}

private fun SemanticsNodeInteraction.assertProgress(value: Float) = assert(
    SemanticsMatcher.expectValue(
        SemanticsProperties.ProgressBarRangeInfo,
        ProgressBarRangeInfo(value, 0f..1f, 0),
    ),
)
