package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w412dp-h916dp-port")
class ArtistContextMenuTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val artist = LibraryArtist("Whole Artist", 3, 2, "content://artists/whole")
    private val otherArtist = LibraryArtist("Other Artist", 1, 1, "content://artists/other")

    @Test
    fun longPressingAnArtistDeletesEveryUnwindowedIdAfterConfirming() {
        val controls = RecordingContextMenuControls()
        composeArtists(controls, selectedArtist = null)

        compose.onNodeWithText("Whole Artist").performTouchInput { longClick() }
        compose.onNodeWithText("Delete from device…").performClick()
        compose.onNodeWithText("Delete 3 tracks from Whole Artist?").assertIsDisplayed()
        compose.onNodeWithText("Cancel").performClick()
        assertEquals(emptyList<List<Long>>(), controls.deleted)

        compose.onNodeWithText("Whole Artist").performTouchInput { longClick() }
        compose.onNodeWithText("Delete from device…").performClick()
        compose.onNodeWithText("Delete", useUnmergedTree = true).performClick()

        assertEquals(listOf(listOf(9L, 7L, 5L)), controls.deleted)
    }

    @Test
    fun longPressingAnArtistPlaysEveryUnwindowedIdInOrder() {
        val controls = RecordingContextMenuControls()
        composeArtists(controls, selectedArtist = null)

        compose.onNodeWithText("Whole Artist").performTouchInput { longClick() }
        compose.onNode(
            hasText("Play") and hasAnyAncestor(hasTestTag("library-track-context-menu")),
        ).performClick()

        assertEquals(listOf(9L, 7L, 5L), controls.playedIds)
        assertEquals(0, controls.playedStartIndex)
    }

    @Test
    fun theArtistPageOverflowDeletesTheWholeArtistAfterConfirming() {
        val controls = RecordingContextMenuControls()
        composeArtists(
            controls,
            selectedArtist = ArtistTrackList(
                artist = artist,
                albums = LibraryWindow(
                    2,
                    listOf(
                        LibraryAlbum("First", artist.name, "content://albums/first", 2, 2026, 0),
                    ),
                    true,
                ),
            ),
        )

        compose.onNodeWithTag("artist-detail-overflow").performClick()
        compose.onNodeWithText("Delete from device…").performClick()
        compose.onNodeWithText("Delete 3 tracks from Whole Artist?").assertIsDisplayed()
        compose.onNodeWithText("Delete", useUnmergedTree = true).performClick()

        // The page has only its first album window loaded; the menu still
        // takes every track the artist has.
        assertEquals(listOf(listOf(9L, 7L, 5L)), controls.deleted)
    }

    private fun composeArtists(
        controls: RecordingContextMenuControls,
        selectedArtist: ArtistTrackList?,
    ) {
        compose.setContent {
            MaterialTheme {
                CompositionLocalProvider(
                    LocalPlaybackControls provides controls,
                    LocalAlbumTrackIds provides { listOf(4L) },
                    LocalArtistTrackIds provides { selected ->
                        if (selected == artist) listOf(9L, 7L, 5L) else listOf(4L)
                    },
                ) {
                    ArtistsTab(
                        surfaceLayout = SurfaceLayout.STACKED,
                        surfaceState = MobileSurfaceViewModel(),
                        artists = LibraryWindow(2, listOf(artist, otherArtist), false),
                        searchText = "",
                        selectedArtist = selectedArtist,
                        playback = PlaybackUiState().libraryPlayback(),
                        openArtist = {},
                        openAlbum = {},
                        closeArtist = {},
                        play = {},
                        lastRequestedOffset = null,
                        artistRequestedOffset = null,
                        loadMoreArtists = {},
                        loadMoreArtistTracks = {},
                    )
                }
            }
        }
    }
}
