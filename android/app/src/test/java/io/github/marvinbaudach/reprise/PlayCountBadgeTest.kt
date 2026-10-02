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
import androidx.compose.ui.test.onNodeWithContentDescription
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
import kotlin.math.abs
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
    fun badgeSlotMatchesSingleDigitAtDoubleFontScaleWithoutMovingTheDuration() {
        val scaledTrack = mutableStateOf(
            track(id = 830, title = "Silent Track", playCount = 0, durationMs = 100_000),
        )
        showTrackRows(
            fontScale = 2f,
            tracks = listOf(scaledTrack.value),
            trackSource = { listOf(scaledTrack.value) },
        )
        val neverPlayedDurationTop = durationTopWithinRow("1:40", trackId = 830)

        // A single digit is the stable control for the reserved badge slot at this scale.
        scaledTrack.value = scaledTrack.value.copy(playCount = 7)
        compose.waitForIdle()

        assertEquals(
            neverPlayedDurationTop,
            durationTopWithinRow("1:40", trackId = 830),
            0.5f,
        )
    }

    @Test
    fun playCountsKeepTheDurationAlignedAtEverySupportedFontScale() {
        val mismatches = mutableListOf<String>()
        val fontScale = mutableStateOf(0.85f)
        val scaledTrack = mutableStateOf(
            track(id = 833, title = "Scaled Track", playCount = 7, durationMs = 103_000),
        )
        showTrackRows(
            fontScaleSource = { fontScale.value },
            tracks = listOf(scaledTrack.value),
            trackSource = { listOf(scaledTrack.value) },
        )

        listOf(0.85f, 1f, 1.3f, 2f).forEach { scale ->
            fontScale.value = scale
            scaledTrack.value = scaledTrack.value.copy(playCount = 7)
            compose.waitForIdle()
            val expectedTop = durationTopWithinRow("1:43", trackId = 833)
            val expectedRight = durationRightWithinRow("1:43", trackId = 833)

            listOf(27L, 127L, 999L, 1_234L, 1_950L, 99_500L, 999_499L, 999_499_999L)
                .forEach { playCount ->
                    scaledTrack.value = scaledTrack.value.copy(playCount = playCount)
                    compose.waitForIdle()

                    val actualTop = durationTopWithinRow("1:43", trackId = 833)
                    val actualRight = durationRightWithinRow("1:43", trackId = 833)
                    if (abs(expectedTop - actualTop) > 0.5f) {
                        mismatches += "$playCount at $scale: top $actualTop, expected $expectedTop"
                    }
                    if (abs(expectedRight - actualRight) > 0.5f) {
                        mismatches += "$playCount at $scale: right $actualRight, expected $expectedRight"
                    }
                }
        }
        assertTrue("duration alignment mismatches: ${mismatches.joinToString()}", mismatches.isEmpty())
    }

    @Test
    fun badgeAnnouncesTheExactCountWithoutExposingVisibleCountText() {
        showTrackRows(
            tracks = listOf(
                track(id = 834, title = "Compact Count", playCount = 1_234, durationMs = 104_000),
                track(id = 835, title = "Plain Count", playCount = 27, durationMs = 105_000),
            ),
        )

        compose.onNodeWithContentDescription("1234 plays", useUnmergedTree = true).assertExists()
        listOf("1.2k", "1234").forEach { countText ->
            compose.onNodeWithText(countText).assertDoesNotExist()
            compose.onNodeWithText(countText, useUnmergedTree = true).assertDoesNotExist()
        }
        compose.onNodeWithText("27").assertDoesNotExist()
        compose.onNodeWithText("27", useUnmergedTree = true).assertDoesNotExist()
    }

    private fun showTrackRows(
        fontScale: Float? = null,
        fontScaleSource: (() -> Float?)? = null,
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
                val selectedFontScale = fontScaleSource?.invoke() ?: fontScale
                if (selectedFontScale == null) {
                    content(trackSource?.invoke() ?: tracks, surfaceState)
                } else {
                    CompositionLocalProvider(
                        LocalDensity provides Density(currentDensity.density, selectedFontScale),
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

    private fun durationRightWithinRow(duration: String, trackId: Long): Float {
        val durationRight = compose.onNodeWithText(duration, useUnmergedTree = true)
            .fetchSemanticsNode()
            .boundsInRoot.right
        val rowLeft = compose.onNodeWithTag("library-track-row-$trackId")
            .fetchSemanticsNode()
            .boundsInRoot.left
        return durationRight - rowLeft
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
