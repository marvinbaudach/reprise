package de.reprise.spike

import androidx.activity.ComponentActivity
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performScrollToIndex
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w412dp-h916dp-port")
class NowPlayingQueueTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun inFlightLoadMoreCannotAppendAfterAutomaticAdvanceReloadsTheQueue() {
        val queue = (1L..202L).map { id -> configurationTestTrack(id, "Queue Track $id") }
        var currentIndex = 0
        val controls = ConfigurationTestPlaybackControls(
            loadUpcoming = { range -> queue.drop(currentIndex + 1).window(range) },
        )
        var playback by mutableStateOf(PlaybackUiState(currentTrackId = 1))

        compose.setContent {
            MaterialTheme {
                CompositionLocalProvider(LocalPlaybackControls provides controls) {
                    NowPlayingQueuePage(
                        playback,
                        MobileSurfaceViewModel(),
                        SurfaceLayout.STACKED,
                    )
                }
            }
        }
        controls.deferUpcomingLoad(offset = 200)
        compose.onNodeWithTag("now-playing-queue").performScrollToIndex(200)
        compose.waitForIdle()
        assertEquals(
            listOf(LibraryWindowRange(0, 200), LibraryWindowRange(200, 1)),
            controls.loadUpcomingRequests,
        )

        compose.runOnIdle {
            currentIndex = 1
            playback = playback.copy(currentTrackId = 2)
        }
        compose.waitForIdle()
        assertEquals(
            listOf(
                LibraryWindowRange(0, 200),
                LibraryWindowRange(200, 1),
                LibraryWindowRange(0, 200),
            ),
            controls.loadUpcomingRequests,
        )
        controls.completeDeferredUpcomingLoads()
        compose.waitForIdle()

        compose.onNodeWithText("200 upcoming tracks").assertIsDisplayed()
        compose.onNodeWithText("201 upcoming tracks").assertDoesNotExist()
    }
}

private fun <T> List<T>.window(range: LibraryWindowRange): LibraryWindow<T> {
    val offset = range.offset.toInt()
    val rows = drop(offset).take(range.limit.toInt())
    return LibraryWindow(
        total = size.toLong(),
        rows = rows,
        hasMore = offset + rows.size < size,
    )
}
