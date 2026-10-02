package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidColorScheme

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h1000dp")
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
    fun successfulCompletionDismissesAfterFourSeconds() {
        var dismissals = 0
        compose.mainClock.autoAdvance = false
        show(progress(ArtistPhotoProgressPhase.COMPLETE, done = 8, total = 8)) {
            dismissals += 1
        }

        compose.mainClock.advanceTimeBy(4_001)
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
    fun animatedSegmentFractionsCannotInvert() {
        assertEquals(0.4f, clampedArtistPhotoDoneFraction(0.8f, 0.4f))
        assertEquals(0f, clampedArtistPhotoDoneFraction(-0.2f, 0.4f))
        assertEquals(1f, clampedArtistPhotoDoneFraction(1.2f, 1.2f))
    }

    private fun show(
        initial: ArtistPhotoProgress?,
        dismiss: () -> Unit = {},
    ) {
        val state = mutableStateOf(initial)
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                ArtistPhotoEdgeProgress(state.value, dismiss)
            }
        }
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
}

private fun androidx.compose.ui.test.SemanticsNodeInteraction.assertProgress(value: Float) = assert(
    SemanticsMatcher.expectValue(
        SemanticsProperties.ProgressBarRangeInfo,
        ProgressBarRangeInfo(value, 0f..1f, 0),
    ),
)
