package de.reprise.spike

import androidx.activity.ComponentActivity
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performTouchInput
import de.reprise.spike.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidColorScheme
import uniffi.reprise_android_ffi.AndroidPlaybackState

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h1000dp")
class NowPlayingGesturesTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun coverDragPastThresholdSkipsToTheNextTrack() {
        val controls = GestureRecordingControls()
        compose.setContent { testNowPlayingSheet(controls = controls) }

        compose.onNodeWithTag("now-playing-gestures").performTouchInput {
            down(Offset(width * 0.75f, height * 0.3f))
            moveTo(Offset(width * 0.35f, height * 0.3f))
            up()
        }

        assertEquals(1, controls.nextCalls)
    }

    @Test
    fun coverDragBelowThresholdSpringsBackWithoutChangingTrack() {
        val controls = GestureRecordingControls()
        compose.setContent { testNowPlayingSheet(controls = controls) }

        compose.onNodeWithTag("now-playing-gestures").performTouchInput {
            down(Offset(width * 0.65f, height * 0.3f))
            moveTo(Offset(width * 0.55f, height * 0.3f))
            up()
        }

        assertEquals(0, controls.nextCalls)
        assertEquals(0, controls.previousCalls)
    }

    @Test
    fun downwardDragClosesTheSheet() {
        var closed = false
        compose.setContent { testNowPlayingSheet(close = { closed = true }) }

        compose.onNodeWithTag("now-playing-gestures").performTouchInput {
            down(Offset(centerX, height * 0.3f))
            moveTo(Offset(centerX, height * 0.55f))
            up()
        }

        assertTrue(closed)
    }

    @Test
    fun doubleTapOnTheLeftSeeksBackTenSecondsAndShowsItsMarker() {
        val controls = GestureRecordingControls()
        compose.setContent { testNowPlayingSheet(controls = controls) }

        compose.onNodeWithTag("now-playing-gestures").performTouchInput {
            val point = Offset(width * 0.25f, height * 0.3f)
            down(point)
            up()
            advanceEventTime(100)
            down(point)
            up()
        }

        assertEquals(listOf(10_000L), controls.seekPositions)
        compose.onNodeWithText("−10 s").assertIsDisplayed()
    }

    @Composable
    private fun testNowPlayingSheet(
        controls: PlaybackControls = DisconnectedPlaybackControls,
        close: () -> Unit = {},
    ) {
        val theme = MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.SYSTEM,
            dynamicAvailable = false,
        )
        RepriseTheme(theme, darkPalette = true) {
            CompositionLocalProvider(LocalPlaybackControls provides controls) {
                NowPlayingSheet(
                    track = gestureTrack(),
                    playback = gesturePlayback(),
                    close = close,
                )
            }
        }
    }
}

private class GestureRecordingControls : PlaybackControls by DisconnectedPlaybackControls {
    val seekPositions = mutableListOf<Long>()
    var nextCalls = 0
        private set
    var previousCalls = 0
        private set

    override fun next() {
        nextCalls += 1
    }

    override fun previous() {
        previousCalls += 1
    }

    override fun seekTo(positionMs: Long) {
        seekPositions += positionMs
    }
}

private fun gesturePlayback() = PlaybackUiState(
    ready = true,
    state = AndroidPlaybackState.PAUSED,
    currentIndex = 0,
    currentTrackId = 830,
    currentTrackUri = "content://provider/document/song.flac",
    positionMs = 20_000,
    durationMs = 100_000,
    playPauseLabel = "Play",
)

private fun gestureTrack() = LibraryTrack(
    id = 830,
    uri = "content://provider/document/song.flac",
    title = "Song",
    artist = "Artist",
    album = "Album",
    durationMs = 100_000,
    playCount = 27,
    rating = 2,
)
