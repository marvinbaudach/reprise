package de.reprise.spike

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.ViewGroup
import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithText
import de.reprise.spike.ui.theme.RepriseTheme
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.reprise_android_ffi.AndroidColorScheme

/** Visibility is a drawing contract: off removes the rating instead of emptying it. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h1000dp")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LibraryRatingVisibilityTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private var ratingsVisible by mutableStateOf(false)

    @Test
    fun offDrawsNoRatingAndOnDrawsTheStoredRating() {
        showTrackRow()

        // The badge as a whole, not the string "4/5": a row that drew an empty
        // star and "0/5" would satisfy the text assertion while showing the
        // listener exactly the thing the switch is meant to remove.
        compose.onAllNodesWithTag(TRACK_RATING_TAG, useUnmergedTree = true).assertCountEquals(0)
        compose.onNodeWithText("4/5").assertDoesNotExist()
        val off = renderSurface()

        ratingsVisible = true
        compose.waitForIdle()
        compose.onAllNodesWithTag(TRACK_RATING_TAG, useUnmergedTree = true).assertCountEquals(1)
        compose.onNodeWithText("4/5").assertIsDisplayed()
        val on = renderSurface()

        val differing = (0 until off.height).sumOf { y ->
            (0 until off.width).count { x -> off[x, y] != on[x, y] }
        }
        assertTrue(
            "showing the stored rating must change real row pixels, but only " +
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
                CompositionLocalProvider(
                    LocalLibraryRatingControl provides LibraryRatingControl(
                        enabled = ratingsVisible,
                        select = {},
                    ),
                ) {
                    TrackRows(
                        surfaceLayout = SurfaceLayout.STACKED,
                        surfaceState = MobileSurfaceViewModel(),
                        listKey = LibraryListKey.TITLES,
                        tracks = LibraryWindow(
                            total = 1,
                            rows = listOf(ratedTrack),
                            hasMore = false,
                        ),
                        playback = PlaybackUiState(),
                        lastRequestedOffset = null,
                        play = {},
                        loadMore = {},
                    )
                }
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

        val ratedTrack = LibraryTrack(
            id = 91,
            uri = "content://provider/document/rated.flac",
            title = "Rated Song",
            artist = "Artist",
            album = "Album",
            durationMs = 120_000,
            playCount = 3,
            rating = 4,
        )
    }
}
