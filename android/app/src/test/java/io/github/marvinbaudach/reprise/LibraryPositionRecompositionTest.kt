package io.github.marvinbaudach.reprise

import android.os.Looper
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidPlaybackSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackState
import uniffi.reprise_android_ffi.AndroidRepeatMode

@RunWith(RobolectricTestRunner::class)
@Config(
    sdk = [36],
    qualifiers = "w412dp-h916dp-port",
    application = LibraryPositionRecompositionTestApplication::class,
)
class LibraryPositionRecompositionTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val application: LibraryPositionRecompositionTestApplication
        get() = RuntimeEnvironment.getApplication() as LibraryPositionRecompositionTestApplication

    @After
    fun releaseTheService() {
        application.releaseService()
    }

    @Test
    fun positionTicksRedrawTheMiniPlayerProgressWithoutRecomposingAVisibleTrackRow() {
        publishPosition(0)
        compose.onNodeWithTag("library-track-row-1").assertExists()
        compose.onNodeWithTag("library-mini-player").assertExists()
        val compositionsBeforeTicks = application.observer.playingRowCompositions
        assertTrue("the visible playing row was never composed", compositionsBeforeTicks >= 1)

        repeat(POSITION_TICK_COUNT) { tick ->
            publishPosition((tick + 1L) * POSITION_TICK_MS)
        }

        assertEquals(
            "position-only updates recomposed the visible library row",
            compositionsBeforeTicks,
            application.observer.playingRowCompositions,
        )
        compose.onNodeWithTag("library-mini-player").captureToImage()
        assertEquals(
            POSITION_TICK_COUNT * POSITION_TICK_MS / TRACK_DURATION_MS.toFloat(),
            application.observer.lastProgress,
            0.0001f,
        )
    }

    private fun publishPosition(positionMs: Long) {
        application.service.publish(playingSnapshot(positionMs))
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
    }
}

internal class LibraryPositionRecompositionTestApplication : ConfigurationTestApplication() {
    val observer = RecordingLibraryPerformanceObserver()

    override fun mainActivitySurface(): MainActivitySurfaceDependencies =
        super.mainActivitySurface().copy(libraryPerformanceObserver = observer)
}

internal class RecordingLibraryPerformanceObserver : LibraryPerformanceObserver {
    var playingRowCompositions = 0
        private set
    var lastProgress = -1f
        private set

    override fun trackRowComposed(trackId: Long, presentation: TrackPlaybackPresentation) {
        if (trackId == PLAYING_TRACK_ID && presentation.isCurrent && presentation.animateBars) {
            playingRowCompositions += 1
        }
    }

    override fun miniPlayerProgressDrawn(progress: Float) {
        lastProgress = progress
    }
}

private fun playingSnapshot(positionMs: Long) = AndroidPlaybackSnapshot(
    state = AndroidPlaybackState.PLAYING,
    currentIndex = 0u,
    currentTrackId = PLAYING_TRACK_ID,
    currentTrackUri = "content://provider/document/$PLAYING_TRACK_ID.flac",
    positionMs = positionMs,
    durationMs = TRACK_DURATION_MS,
    automaticAdvanceCount = 0u,
    shuffled = false,
    repeat = AndroidRepeatMode.OFF,
    error = null,
)

private const val PLAYING_TRACK_ID = 1L
private const val POSITION_TICK_COUNT = 20
private const val POSITION_TICK_MS = 5_000L
private const val TRACK_DURATION_MS = 120_000L
