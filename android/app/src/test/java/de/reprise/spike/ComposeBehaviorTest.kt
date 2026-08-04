package de.reprise.spike

import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.test.performTouchInput
import de.reprise.spike.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidPlaybackState

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h1000dp")
class ComposeBehaviorTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun seekDragOwnsTheHeadUntilReleaseThenSnapshotsResume() {
        val controls = RecordingPlaybackControls()
        var playback by mutableStateOf(testPlayback(positionMs = 20_000))
        compose.setContent {
            RepriseTheme {
                CompositionLocalProvider(LocalPlaybackControls provides controls) {
                    NowPlayingSheet(
                        track = testTrack(rating = 2),
                        playback = playback,
                        close = {},
                    )
                }
            }
        }
        val slider = compose.onNode(SemanticsMatcher.keyIsDefined(SemanticsProperties.ProgressBarRangeInfo))

        slider.performTouchInput {
            down(Offset(width * 0.25f, centerY))
            (1..5).forEach { step ->
                advanceEventTime(20)
                moveTo(Offset(width * (0.25f + step * 0.1f), centerY))
            }
        }
        val draggedPosition = slider.progress()

        compose.runOnIdle { playback = testPlayback(positionMs = 10_000) }
        assertEquals(draggedPosition, slider.progress(), 0.5f)

        slider.performTouchInput { up() }
        assertTrue(controls.seekPositions.isNotEmpty())
        compose.runOnIdle { playback = testPlayback(positionMs = 30_000) }
        assertEquals(30_000f, slider.progress(), 0.5f)
    }

    @Test
    fun failedRatingShowsTheErrorWithoutMovingTheStars() {
        val failure = "Could not save rating: track is missing."
        val controls = RecordingPlaybackControls(ratingFailure = failure)
        compose.setContent {
            RepriseTheme {
                CompositionLocalProvider(LocalPlaybackControls provides controls) {
                    NowPlayingSheet(
                        track = testTrack(rating = 2),
                        playback = testPlayback(positionMs = 20_000),
                        close = {},
                    )
                }
            }
        }

        compose.onNodeWithContentDescription("Rate 2 of 5 stars").assertIsSelected()
        compose.onNodeWithContentDescription("Rate 3 of 5 stars").assertIsNotSelected()

        compose.onNodeWithContentDescription("Rate 4 of 5 stars").performClick()

        compose.onNodeWithText(failure).assertIsDisplayed()
        compose.onNodeWithContentDescription("Rate 2 of 5 stars").assertIsSelected()
        compose.onNodeWithContentDescription("Rate 3 of 5 stars").assertIsNotSelected()
        compose.onNodeWithContentDescription("Rate 4 of 5 stars").assertIsNotSelected()
        assertEquals(listOf(830L to 4), controls.ratingRequests)
    }

    @Test
    fun miniPlayerOpensTheSheetBackClosesItAndTheLibraryKeepsWorking() {
        val tracks = listOf(
            testTrack(rating = 2).copy(id = 830, title = "First Song"),
            testTrack(rating = 4).copy(
                id = 831,
                uri = "content://provider/document/second.flac",
                title = "Second Song",
            ),
        )
        val browse = LibraryScreenState.Browse(
            titles = LibraryWindow(total = 2, rows = tracks, hasMore = false),
            albums = LibraryWindow.empty(),
            artists = LibraryWindow.empty(),
        )
        val playedIndices = mutableListOf<Int>()
        var playback by mutableStateOf(testPlayback(positionMs = 20_000))
        compose.setContent {
            RepriseTheme {
                BrowseScreen(
                    state = browse,
                    playback = playback,
                    chooseFolder = {},
                    rescan = {},
                    searchTitles = { _, _ -> browse.titles },
                    listAlbums = { browse.albums },
                    listArtists = { browse.artists },
                    openAlbum = { error("Album navigation is outside this test") },
                    listAlbumTracks = { _, _ -> LibraryWindow.empty() },
                    playTracks = { selection, _ ->
                        playedIndices += selection.startIndex
                        playback = playback.copy(currentIndex = selection.startIndex)
                    },
                )
            }
        }

        compose.onNodeWithText("First Song").performClick()
        compose.onNodeWithContentDescription("Open Now Playing").performClick()
        compose.onNodeWithContentDescription("Collapse Now Playing").assertIsDisplayed()
        compose.onAllNodesWithText("First Song").assertCountEquals(3)

        // The full-screen sheet should intercept pointer hit testing. Calling
        // the retained row's public action proves its state and wiring stayed
        // alive underneath rather than replacing the Library composition.
        compose.onNodeWithText("Second Song")
            .performSemanticsAction(SemanticsActions.OnClick) { click -> click() }
        compose.onAllNodesWithText("Second Song").assertCountEquals(3)
        assertEquals(listOf(0, 1), playedIndices)

        compose.runOnIdle { compose.activity.onBackPressedDispatcher.onBackPressed() }
        compose.onNodeWithContentDescription("Collapse Now Playing").assertDoesNotExist()
        compose.onAllNodesWithText("Second Song").assertCountEquals(2)
    }

    private fun androidx.compose.ui.test.SemanticsNodeInteraction.progress(): Float =
        fetchSemanticsNode().config[SemanticsProperties.ProgressBarRangeInfo].current
}

private fun testPlayback(positionMs: Long) = PlaybackUiState(
    ready = true,
    state = AndroidPlaybackState.PAUSED,
    currentIndex = 0,
    positionMs = positionMs,
    durationMs = 100_000,
    playPauseLabel = "Play",
)

private fun testTrack(rating: Int) = LibraryTrack(
    id = 830,
    uri = "content://provider/document/song.flac",
    title = "Song",
    artist = "Artist",
    album = "Album",
    durationMs = 100_000,
    playCount = 27,
    rating = rating,
)

private class RecordingPlaybackControls(
    private val ratingFailure: String? = null,
) : PlaybackControls by DisconnectedPlaybackControls {
    val seekPositions = mutableListOf<Long>()
    val ratingRequests = mutableListOf<Pair<Long, Int>>()

    override fun seekTo(positionMs: Long) {
        seekPositions += positionMs
    }

    override fun setRating(trackId: Long, rating: Int): String? {
        ratingRequests += trackId to rating
        return ratingFailure
    }
}
