package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.assertContentDescriptionEquals
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.unit.Density
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.reprise_android_ffi.AndroidColorScheme
import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.roundToInt

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h1000dp")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class PlayCountBadgeTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private var rowBackground = Color.Unspecified
    private var secondaryContainer = Color.Unspecified

    @Test
    fun neverPlayedTrackOmitsItsBadgeWithoutMovingTheDuration() {
        showTrackRows()

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

        val neverPlayedBadge = badgeRegion(trackId = 830, duration = "1:40")
        assertEquals(
            "a never-played row must contain no secondary-container badge pixels",
            0,
            neverPlayedBadge.count { it == secondaryContainer },
        )
        assertTrue(
            "the reserved badge region must draw exactly like the row background",
            neverPlayedBadge.all { it == rowBackground },
        )
        val playedBadge = badgeRegion(trackId = 832, duration = "1:42")
        assertTrue(
            "the played-row control must contain secondary-container badge pixels",
            playedBadge.any { it == secondaryContainer },
        )
    }

    @Test
    fun badgeSlotFollowsDoubleFontScaleWithoutMovingTheDuration() {
        val scaledTrack = mutableStateOf(
            track(id = 830, title = "Silent Track", playCount = 0, durationMs = 100_000),
        )
        showTrackRows(
            fontScale = 2f,
            tracks = listOf(scaledTrack.value),
            trackSource = { listOf(scaledTrack.value) },
        )
        val neverPlayedDurationTop = durationTopWithinRow("1:40", trackId = 830)

        scaledTrack.value = scaledTrack.value.copy(playCount = 27)
        compose.waitForIdle()

        assertEquals(
            neverPlayedDurationTop,
            durationTopWithinRow("1:40", trackId = 830),
            0.5f,
        )
    }

    private fun showTrackRows(
        fontScale: Float? = null,
        tracks: List<LibraryTrack> = listOf(
            track(id = 830, title = "Silent Track", playCount = 0, durationMs = 100_000),
            track(id = 831, title = "Once Heard", playCount = 1, durationMs = 101_000),
            track(id = 832, title = "Often Heard", playCount = 27, durationMs = 102_000),
        ),
        trackSource: (() -> List<LibraryTrack>)? = null,
    ) {
        val surfaceState = MobileSurfaceViewModel()
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                rowBackground = MaterialTheme.colorScheme.background
                secondaryContainer = MaterialTheme.colorScheme.secondaryContainer
                val currentDensity = LocalDensity.current
                val content: @Composable (List<LibraryTrack>, MobileSurfaceViewModel) -> Unit =
                    { visibleTracks, surfaceState ->
                        TrackRows(
                            surfaceLayout = SurfaceLayout.STACKED,
                            surfaceState = surfaceState,
                            listKey = LibraryListKey.TITLES,
                            tracks = LibraryWindow(
                                total = visibleTracks.size.toLong(),
                                rows = visibleTracks,
                                hasMore = false,
                            ),
                            playback = PlaybackUiState().libraryPlayback(),
                            lastRequestedOffset = null,
                            play = {},
                            loadMore = {},
                        )
                    }
                if (fontScale == null) {
                    content(trackSource?.invoke() ?: tracks, surfaceState)
                } else {
                    CompositionLocalProvider(
                        LocalDensity provides Density(currentDensity.density, fontScale),
                    ) {
                        content(trackSource?.invoke() ?: tracks, surfaceState)
                    }
                }
            }
        }
        compose.waitForIdle()
    }

    private fun badgeRegion(trackId: Long, duration: String): List<Color> {
        val row = compose.onNodeWithTag("library-track-row-$trackId")
        val rowBounds = row.fetchSemanticsNode().boundsInRoot
        val durationBounds = compose.onNodeWithText(duration, useUnmergedTree = true)
            .fetchSemanticsNode()
            .boundsInRoot
        val pixels = row.captureToImage().toPixelMap()
        val pixelsPerDp = pixels.width / SCREEN_WIDTH_DP
        val right = ceil(durationBounds.right - rowBounds.left).toInt().coerceAtMost(pixels.width)
        val left = (right - TRAILING_COLUMN_DP * pixelsPerDp).roundToInt().coerceAtLeast(0)
        val bottom = floor(
            durationBounds.top - rowBounds.top - BADGE_DURATION_GAP_DP * pixelsPerDp,
        ).toInt().coerceAtLeast(0)
        return (0 until bottom).flatMap { y ->
            (left until right).map { x -> pixels[x, y] }
        }
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
        const val SCREEN_WIDTH_DP = 500f
        const val TRAILING_COLUMN_DP = 48f
        const val BADGE_DURATION_GAP_DP = 2f

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
