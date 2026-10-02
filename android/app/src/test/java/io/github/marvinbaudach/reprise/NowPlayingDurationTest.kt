package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidColorScheme
import uniffi.reprise_android_ffi.AndroidPlaybackState

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w916dp-h412dp-land")
class NowPlayingDurationTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun remainingTimePrefersThePlayerThenFallsBackToTheTrack() {
        assertEquals("−1:20", remainingLabel(20_000, 100_000, 58_000))
        assertEquals("−0:58", remainingLabel(0, 0, 58_000))
        assertEquals("--:--", remainingLabel(0, 0, 0))
    }

    @Test
    fun aPausedUnpreparedTrackShowsItsLengthWithoutEnablingSeek() {
        assertPausedUnpreparedTrackLength(SurfaceLayout.WIDE_SHORT)
    }

    @Test
    fun aPausedUnpreparedTrackShowsItsLengthInTheStackedScene() {
        assertPausedUnpreparedTrackLength(SurfaceLayout.STACKED)
    }

    private fun assertPausedUnpreparedTrackLength(surfaceLayout: SurfaceLayout) {
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                NowPlayingSheet(
                    track = track,
                    playback = playback,
                    surfaceLayout = surfaceLayout,
                    close = {},
                )
            }
        }

        compose.onNodeWithText("−0:58").assertIsDisplayed()
        compose.onNodeWithTag("now-playing-seek").assertIsNotEnabled()
    }

    private companion object {
        val theme = MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.SYSTEM,
            dynamicAvailable = false,
        )
        val track = LibraryTrack(
            id = 1,
            uri = "content://provider/document/paused.flac",
            title = "Paused",
            artist = "Artist",
            album = "Album",
            durationMs = 58_000,
            playCount = 0,
            rating = 0,
        )
        val playback = PlaybackUiState(
            ready = true,
            state = AndroidPlaybackState.PAUSED,
            currentIndex = 0,
            currentTrackId = track.id,
            currentTrackUri = track.uri,
            durationMs = 0,
        )
    }
}
