package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.assertContentDescriptionEquals
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidColorScheme

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h1000dp")
class PlayCountBadgeTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun neverPlayedTrackOmitsItsBadgeWithoutMovingTheDuration() {
        val surfaceState = MobileSurfaceViewModel()
        val tracks = listOf(
            track(id = 830, title = "Silent Track", playCount = 0, durationMs = 100_000),
            track(id = 831, title = "Once Heard", playCount = 1, durationMs = 101_000),
            track(id = 832, title = "Often Heard", playCount = 27, durationMs = 102_000),
        )
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                TrackRows(
                    surfaceLayout = SurfaceLayout.STACKED,
                    surfaceState = surfaceState,
                    listKey = LibraryListKey.TITLES,
                    tracks = LibraryWindow(total = 3, rows = tracks, hasMore = false),
                    playback = PlaybackUiState().libraryPlayback(),
                    lastRequestedOffset = null,
                    play = {},
                    loadMore = {},
                )
            }
        }

        compose.onNodeWithText("0").assertDoesNotExist()
        val neverPlayedDescriptions = compose.onNodeWithText("Silent Track")
            .fetchSemanticsNode()
            .config
            .getOrNull(SemanticsProperties.ContentDescription)
            .orEmpty()
        assertFalse(neverPlayedDescriptions.any { it.contains("play", ignoreCase = true) })
        compose.onNodeWithText("Once Heard").assertContentDescriptionEquals("1 play")

        assertEquals(
            durationTopWithinRow("1:40", trackId = 830),
            durationTopWithinRow("1:42", trackId = 832),
            0.5f,
        )
    }

    private fun durationTopWithinRow(duration: String, trackId: Long): Float {
        val durationTop = compose.onNodeWithText(duration, useUnmergedTree = true)
            .fetchSemanticsNode()
            .boundsInRoot.top
        val rowTop = compose.onNodeWithTag("library-track-row-$trackId")
            .fetchSemanticsNode()
            .boundsInRoot.top
        return durationTop - rowTop
    }

    private companion object {
        val theme = MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.SYSTEM,
            dynamicAvailable = false,
        )

        fun track(id: Long, title: String, playCount: Long, durationMs: Long) = LibraryTrack(
            id = id,
            uri = "content://provider/document/$id.flac",
            title = title,
            artist = "Artist",
            album = "Album",
            durationMs = durationMs,
            playCount = playCount,
            rating = 0,
        )
    }
}
