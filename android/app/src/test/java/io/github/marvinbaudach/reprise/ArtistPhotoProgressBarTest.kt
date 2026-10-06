package io.github.marvinbaudach.reprise

import android.graphics.Bitmap
import android.graphics.Canvas as AndroidCanvas
import android.view.ViewGroup
import androidx.activity.ComponentActivity
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.lifecycle.ViewModelStore
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import java.util.concurrent.ConcurrentLinkedQueue
import kotlin.math.roundToInt
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.reprise_android_ffi.AndroidColorScheme

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h1000dp")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ArtistPhotoProgressBarTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun preparingUsesIndeterminateEdgeProgress() {
        show(progress(ArtistPhotoProgressPhase.PREPARING, total = 0))

        compose.onNodeWithTag("artist-photo-progress-track")
            .assertIsDisplayed()
            .assert(
                SemanticsMatcher.expectValue(
                    SemanticsProperties.ProgressBarRangeInfo,
                    ProgressBarRangeInfo.Indeterminate,
                ),
            )
    }

    @Test
    fun runningUsesDeterminateEdgeProgress() {
        show(progress(ArtistPhotoProgressPhase.RUNNING, done = 2, total = 8))

        compose.onNodeWithTag("artist-photo-progress-track").assertProgress(0.25f)
    }

    @Test
    fun determinateProgressHasOneCountAnnouncement() {
        show(progress(ArtistPhotoProgressPhase.RUNNING, done = 2, total = 8))

        compose.onNodeWithTag("artist-photo-progress-track")
            .assert(
                SemanticsMatcher.expectValue(
                    SemanticsProperties.StateDescription,
                    "Artwork, 2 of 8 downloaded",
                ),
            )
            .assert(SemanticsMatcher.keyNotDefined(SemanticsProperties.ContentDescription))
    }

    @Test
    fun waitingUsesIndeterminateEdgeProgress() {
        show(progress(ArtistPhotoProgressPhase.PAUSED, done = 2, total = 8))

        compose.onNodeWithTag("artist-photo-progress-track")
            .assertIsDisplayed()
            .assert(
                SemanticsMatcher.expectValue(
                    SemanticsProperties.ProgressBarRangeInfo,
                    ProgressBarRangeInfo.Indeterminate,
                ),
            )
    }

    @Test
    fun failedCompletionFillsTheTrackAndKeepsItsCountInTheSummary() {
        val update = progress(
            ArtistPhotoProgressPhase.COMPLETE,
            done = 6,
            failed = 2,
            total = 8,
        )
        show(update)

        compose.onNodeWithTag("artist-photo-progress-track").assertProgress(1f)
        assertEquals(" · 2 without a photo", artistPhotoProgressSummarySuffix(update))
    }

    @Test
    fun failedCompletionRendersPrimaryThenTertiaryAtTheMeasuredSplit() {
        show(
            progress(
                ArtistPhotoProgressPhase.COMPLETE,
                done = 6,
                failed = 2,
                total = 8,
            ),
        )

        val track = compose.onNodeWithTag("artist-photo-progress-track")
            .getUnclippedBoundsInRoot()
        val pixels = renderActivity()
        val density = compose.activity.resources.displayMetrics.density
        val middleY = (
            (track.top.value + (track.bottom.value - track.top.value) / 2f) * density
        ).roundToInt()
        val left = (track.left.value * density).roundToInt()
        val width = ((track.right.value - track.left.value) * density).roundToInt()

        assertColorNear(Color(0xFF4FDBD4), pixels[left + width / 2, middleY])
        assertColorNear(Color(0xFF9184D9), pixels[left + (width * 90) / 100, middleY])
    }

    @Test
    fun noRunCreatesNoEdgeNode() {
        show(null)

        compose.onNodeWithTag("artist-photo-progress-track").assertDoesNotExist()
    }

    @Test
    fun dismissalSticksForOneRunAndClearsForTheNext() {
        val viewModel = MobileSurfaceViewModel()
        viewModel.acceptArtistPhotoProgress(progress(ArtistPhotoProgressPhase.RUNNING, runId = 9))
        viewModel.dismissArtistPhotoProgress()
        viewModel.acceptArtistPhotoProgress(
            progress(ArtistPhotoProgressPhase.RUNNING, runId = 9, done = 2),
        )
        assertNull(viewModel.visibleArtistPhotoProgress)

        viewModel.acceptArtistPhotoProgress(progress(ArtistPhotoProgressPhase.RUNNING, runId = 10))
        assertEquals(10L, viewModel.visibleArtistPhotoProgress?.runId)
    }

    @Test
    fun aBackgroundSnapshotIsPostedBeforeItMutatesComposeState() {
        val posted = ConcurrentLinkedQueue<() -> Unit>()
        var snapshot = progress(ArtistPhotoProgressPhase.RUNNING, runId = 20)
        val viewModel = MobileSurfaceViewModel()
        viewModel.bindArtistPhotoBackfill(
            snapshot = { snapshot },
            start = {},
            cancel = {},
            postToMain = posted::add,
        )
        posted.remove().invoke()
        assertEquals(20L, viewModel.visibleArtistPhotoProgress?.runId)
        snapshot = progress(ArtistPhotoProgressPhase.RUNNING, runId = 21)

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
            snapshot = { progress(ArtistPhotoProgressPhase.RUNNING, runId = 22) },
            start = { starts += 1 },
            cancel = { cancels += 1 },
        )
        ViewModelStore().apply {
            put("surface", viewModel)
            clear()
        }
        assertEquals(1, cancels)

        viewModel.startArtistPhotoBackfill()

        assertEquals(0, starts)
        assertEquals(1, cancels)
    }

    @Test
    fun successfulCompletionDismissesAfterFourSeconds() {
        var dismissals = 0
        compose.mainClock.autoAdvance = false
        show(progress(ArtistPhotoProgressPhase.COMPLETE, done = 8, total = 8)) {
            dismissals += 1
        }

        compose.mainClock.advanceTimeBy(3_900)
        compose.waitForIdle()
        assertEquals(0, dismissals)
        compose.mainClock.advanceTimeBy(101)
        compose.waitForIdle()
        assertEquals(1, dismissals)
    }

    @Test
    fun failedCompletionDismissesAfterTenSeconds() {
        var dismissals = 0
        compose.mainClock.autoAdvance = false
        show(
            progress(
                ArtistPhotoProgressPhase.COMPLETE,
                done = 6,
                failed = 2,
                total = 8,
            ),
        ) { dismissals += 1 }

        compose.mainClock.advanceTimeBy(4_001)
        compose.waitForIdle()
        assertEquals(0, dismissals)
        compose.mainClock.advanceTimeBy(6_000)
        compose.waitForIdle()
        assertEquals(1, dismissals)
    }

    @Test
    fun failedCompletionStillLeavesAfterTheTrackLeavesAndReentersComposition() {
        var dismissals = 0
        val present = mutableStateOf(true)
        val update = progress(
            ArtistPhotoProgressPhase.COMPLETE,
            runId = 14,
            done = 6,
            failed = 2,
            total = 8,
        )
        compose.mainClock.autoAdvance = false
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                if (present.value) {
                    ArtistPhotoEdgeProgress(update, dismiss = { dismissals += 1 })
                }
            }
        }

        compose.mainClock.advanceTimeBy(5_000)
        compose.runOnUiThread { present.value = false }
        compose.mainClock.advanceTimeBy(1_000)
        compose.runOnUiThread { present.value = true }
        compose.mainClock.advanceTimeBy(10_001)
        compose.waitForIdle()

        assertEquals(1, dismissals)
    }

    @Test
    fun trackRemainsDuringItsExitFade() {
        compose.mainClock.autoAdvance = false
        val state = show(progress(ArtistPhotoProgressPhase.RUNNING, done = 2, total = 8))
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeBy(DELETION_LINE_FADE_MS.toLong() + 1)
        val track = compose.onNodeWithTag("artist-photo-progress-track")
            .assertIsDisplayed()
            .getUnclippedBoundsInRoot()
        val density = compose.activity.resources.displayMetrics.density
        val sampleX = (
            (track.left.value + (track.right.value - track.left.value) / 10f) * density
        ).roundToInt()
        val sampleY = (
            (track.top.value + (track.bottom.value - track.top.value) / 2f) * density
        ).roundToInt()
        val backgroundY = ((track.bottom.value + 8f) * density).roundToInt()

        compose.runOnIdle { state.value = null }
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeBy(DELETION_LINE_FADE_MS.toLong() / 2)
        compose.onNodeWithTag("artist-photo-progress-track").assertExists()
        val pixels = renderActivity()
        assertTrue(
            "the fading track must still draw its last progress",
            colorDistance(pixels[sampleX, sampleY], pixels[sampleX, backgroundY]) > 0.05f,
        )

        compose.mainClock.autoAdvance = true
        compose.waitForIdle()
        compose.onNodeWithTag("artist-photo-progress-track").assertDoesNotExist()
    }

    @Test
    fun animatedSegmentFractionsCannotInvert() {
        assertEquals(0.4f, clampedArtistPhotoDoneFraction(0.8f, 0.4f))
        assertEquals(0f, clampedArtistPhotoDoneFraction(-0.2f, 0.4f))
        assertEquals(1f, clampedArtistPhotoDoneFraction(1.2f, 1.2f))
    }

    private fun show(
        initial: ArtistPhotoProgress?,
        dismiss: () -> Unit = {},
    ): androidx.compose.runtime.MutableState<ArtistPhotoProgress?> {
        val state = mutableStateOf(initial)
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                ArtistPhotoEdgeProgress(state.value, dismiss)
            }
        }
        return state
    }

    private fun progress(
        phase: ArtistPhotoProgressPhase,
        runId: Long = 1,
        done: Long = 0,
        failed: Long = 0,
        total: Long = 8,
    ) = ArtistPhotoProgress(runId, phase, done, failed, total)

    private val theme = MobileThemeSelection(
        palette = MobileTheme.NOCTURNE,
        colorScheme = AndroidColorScheme.SYSTEM,
        dynamicAvailable = false,
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

    private fun colorDistance(first: Color, second: Color): Float =
        kotlin.math.abs(first.red - second.red) +
            kotlin.math.abs(first.green - second.green) +
            kotlin.math.abs(first.blue - second.blue)
}

private fun androidx.compose.ui.test.SemanticsNodeInteraction.assertProgress(value: Float) = assert(
    SemanticsMatcher.expectValue(
        SemanticsProperties.ProgressBarRangeInfo,
        ProgressBarRangeInfo(value, 0f..1f, 0),
    ),
)
