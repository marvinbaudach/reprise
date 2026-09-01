package io.github.marvinbaudach.reprise

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.ViewGroup
import androidx.activity.ComponentActivity
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithContentDescription
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.reprise_android_ffi.AndroidColorScheme

/** The library heart is always drawn, and its two states differ in real pixels. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h1000dp")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LibraryRatingVisibilityTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private var rating by mutableIntStateOf(0)

    @Test
    fun rowAlwaysDrawsOneHeartAndFiveFillsIt() {
        showTrackRow()

        compose.onAllNodesWithTag(TRACK_HEART_TAG, useUnmergedTree = true).assertCountEquals(1)
        compose.onNodeWithContentDescription("Add to favourites").assertExists()
        val ordinary = renderSurface()

        rating = 5
        compose.waitForIdle()
        compose.onAllNodesWithTag(TRACK_HEART_TAG, useUnmergedTree = true).assertCountEquals(1)
        compose.onNodeWithContentDescription("Remove from favourites").assertExists()
        val favourite = renderSurface()

        val differing = (0 until ordinary.height).sumOf { y ->
            (0 until ordinary.width).count { x -> ordinary[x, y] != favourite[x, y] }
        }
        assertTrue(
            "changing only the favourite state must change real row pixels, but only " +
                "$differing pixels changed",
            differing >= MINIMUM_CHANGED_PIXELS,
        )
    }

    private fun showTrackRow() {
        val theme = MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.SYSTEM,
            dynamicAvailable = false,
        )
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                TrackRows(
                    surfaceLayout = SurfaceLayout.STACKED,
                    surfaceState = MobileSurfaceViewModel(),
                    listKey = LibraryListKey.TITLES,
                    tracks = LibraryWindow(
                        total = 1,
                        rows = listOf(track.copy(rating = rating)),
                        hasMore = false,
                    ),
                    playback = PlaybackUiState().libraryPlayback(),
                    lastRequestedOffset = null,
                    play = {},
                    loadMore = {},
                )
            }
        }
        compose.waitForIdle()
    }

    private fun renderSurface(): androidx.compose.ui.graphics.PixelMap {
        val content = compose.activity.findViewById<ViewGroup>(android.R.id.content)
        val bitmap = Bitmap.createBitmap(content.width, content.height, Bitmap.Config.ARGB_8888)
        content.draw(Canvas(bitmap))
        return bitmap.asImageBitmap().toPixelMap()
    }

    private companion object {
        const val MINIMUM_CHANGED_PIXELS = 20

        val track = LibraryTrack(
            id = 91,
            uri = "content://provider/document/rated.flac",
            title = "Rated Song",
            artist = "Artist",
            album = "Album",
            durationMs = 120_000,
            playCount = 3,
            rating = 0,
        )
    }
}
