package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToIndex
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidColorScheme

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w412dp-h916dp-port")
class ArtistDetailHeadTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun art_3_thePortraitShimmerFollowsAndFadesWithTheHead() {
        assertEquals(
            ArtistHeadShimmerTransform(translationY = 0f, alpha = 1f),
            artistHeadShimmerTransform(firstVisibleIndex = 0, offsetPx = 0, headHeightPx = 300),
        )
        assertEquals(
            ArtistHeadShimmerTransform(translationY = -75f, alpha = 0.75f),
            artistHeadShimmerTransform(firstVisibleIndex = 0, offsetPx = 75, headHeightPx = 300),
        )
        assertEquals(
            0f,
            artistHeadShimmerTransform(
                firstVisibleIndex = 1,
                offsetPx = 0,
                headHeightPx = 300,
            ).alpha,
        )
    }

    @Test
    fun art_3_thePlayButtonScrollsAwayWithThePortrait() {
        showLongArtistFromTheList()

        compose.onNodeWithText(ARTIST_NAME).performClick()
        compose.onNodeWithTag("library-artist-albums-list").performScrollToIndex(12)

        compose.onNodeWithTag("artist-detail-play").assertDoesNotExist()
        compose.onNodeWithTag("artist-portrait-head-image", useUnmergedTree = true)
            .assertDoesNotExist()
        compose.onNodeWithContentDescription("Back to artists").assertIsDisplayed()
    }

    @Test
    fun art_3_theTitleRowTakesItsContainerOnlyOnceTheListScrolls() {
        showLongArtistFromTheList()
        compose.onNodeWithText(ARTIST_NAME).performClick()
        val titleRow = compose.onNodeWithTag("artist-detail-header")
        val list = compose.onNodeWithTag("library-artist-albums-list")

        titleRow.assert(SemanticsMatcher.expectValue(ArtistHeaderScrolledKey, false))
        list.performScrollToIndex(12)
        titleRow.assert(SemanticsMatcher.expectValue(ArtistHeaderScrolledKey, true))
        list.performScrollToIndex(0)
        titleRow.assert(SemanticsMatcher.expectValue(ArtistHeaderScrolledKey, false))
    }

    private fun showLongArtistFromTheList() {
        val albums = (1..20).map { index ->
            LibraryAlbum(
                title = "Album $index",
                artist = ARTIST_NAME,
                representativeUri = "content://albums/$index",
                trackCount = 2,
                year = 2026 - index,
                totalDurationMs = 120_000,
            )
        }
        val artist = LibraryArtist(
            name = ARTIST_NAME,
            trackCount = albums.sumOf(LibraryAlbum::trackCount),
            albumCount = albums.size.toLong(),
            representativeUri = "content://artists/scrollable",
        )
        val detail = ArtistTrackList(
            artist = artist,
            albums = window(albums),
            untaggedTracks = LibraryWindow.empty(),
        )
        val surfaceState = MobileSurfaceViewModel()
        compose.setContent {
            RepriseTheme(THEME, darkPalette = true) {
                var selected by remember { mutableStateOf<ArtistTrackList?>(null) }
                ArtistsTab(
                    surfaceLayout = SurfaceLayout.STACKED,
                    surfaceState = surfaceState,
                    artists = window(listOf(artist)),
                    searchText = "",
                    selectedArtist = selected,
                    playback = PlaybackUiState().libraryPlayback(),
                    openArtist = { selected = detail },
                    closeArtist = { selected = null },
                    play = {},
                    lastRequestedOffset = null,
                    artistRequestedOffset = null,
                    loadMoreArtists = {},
                    loadMoreArtistTracks = {},
                )
            }
        }
    }

    private fun <T> window(rows: List<T>) = LibraryWindow(
        total = rows.size.toLong(),
        rows = rows,
        hasMore = false,
    )

    private companion object {
        const val ARTIST_NAME = "Scrollable Artist"
        val THEME = MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.SYSTEM,
            dynamicAvailable = false,
        )
    }
}
