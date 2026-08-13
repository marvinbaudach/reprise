package de.reprise.spike

import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import de.reprise.spike.ui.theme.RepriseTheme
import org.junit.Rule
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidColorScheme

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h1000dp")
class ArtistDetailSurfaceTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val theme = MobileThemeSelection(
        palette = MobileTheme.NOCTURNE,
        colorScheme = AndroidColorScheme.SYSTEM,
        dynamicAvailable = false,
    )

    @Test
    fun artistPageListsTheArtistsAlbums() {
        showArtist(
            detail = artistDetail(
                albums = listOf(
                    album("Newest", 2024),
                    album("Earlier", 2020),
                ),
            ),
        )

        compose.onNodeWithText("Albums").assertIsDisplayed()
        compose.onNodeWithTag("library-artist-albums-list").assertIsDisplayed()
        compose.onNodeWithText("Newest").assertIsDisplayed()
        compose.onNodeWithText("Earlier").assertIsDisplayed()
    }

    @Test
    fun artistAlbumsRenderNewestFirstWithAlphabeticalTies() {
        showArtist(
            artistDetail(
                albums = listOf(
                    album("Alpha", 2024),
                    album("Zed", 2024),
                    album("Old", 2010),
                ),
            ),
        )

        val alpha = compose.onNodeWithText("Alpha").getUnclippedBoundsInRoot().top
        val zed = compose.onNodeWithText("Zed").getUnclippedBoundsInRoot().top
        val old = compose.onNodeWithText("Old").getUnclippedBoundsInRoot().top
        assertTrue(alpha < zed)
        assertTrue(zed < old)
    }

    @Test
    fun artistAlbumRowOpensTheNestedAlbumPage() {
        val opened = mutableListOf<LibraryAlbum>()
        val target = album("Things We Lost in the Fire", 2001)
        showArtist(
            detail = artistDetail(albums = listOf(target)),
            openAlbum = opened::add,
            openedAlbumTracks = listOf(track("Dinosaur Act")),
        )

        compose.onNodeWithText(target.title).performClick()

        assertEquals(listOf(target), opened)
        compose.onNodeWithContentDescription("Back").assertIsDisplayed()
        compose.onNodeWithText("Dinosaur Act").assertIsDisplayed()
    }

    @Test
    fun artistWithUntaggedTracksShowsOtherTitles() {
        showArtist(artistDetail(untagged = listOf(track("Laser Beam"))))
        compose.onNodeWithText("Other titles").assertIsDisplayed()
        compose.onNodeWithText("Laser Beam").assertIsDisplayed()
    }

    @Test
    fun artistWithoutUntaggedTracksHasNoOtherTitlesSection() {
        showArtist(artistDetail(albums = listOf(album("Hey What", 2021))))
        compose.onNodeWithText("Other titles").assertDoesNotExist()
    }

    @Test
    fun artistWithoutAlbumsStillShowsOtherTitles() {
        showArtist(artistDetail(untagged = listOf(track("Loose Song"))))

        compose.onNodeWithText("Albums").assertDoesNotExist()
        compose.onNodeWithText("Other titles").assertIsDisplayed()
        compose.onNodeWithText("Loose Song").assertIsDisplayed()
    }

    @Test
    fun artistPlayUsesAlbumsInPageOrderThenOtherTitles() {
        val firstAlbum = album("Newest", 2024)
        val secondAlbum = album("Earlier", 2020)
        val loose = track("Loose Song")
        val selections = mutableListOf<Pair<List<Long>, Int>>()
        val controls = object : PlaybackControls by DisconnectedPlaybackControls {
            override fun playTrackIds(trackIds: List<Long>, startIndex: Int) {
                selections += trackIds to startIndex
            }
        }
        showArtist(
            detail = artistDetail(
                albums = listOf(firstAlbum, secondAlbum),
                untagged = listOf(loose),
            ),
            controls = controls,
            albumTrackIds = { album ->
                when (album) {
                    firstAlbum -> listOf(11L, 12L)
                    secondAlbum -> listOf(21L)
                    else -> emptyList()
                }
            },
        )

        compose.onNodeWithContentDescription("Play Low").performClick()

        assertEquals(listOf(listOf(11L, 12L, 21L, loose.id) to 0), selections)
    }

    @Test
    fun playingFromAnArtistAlbumUsesTheAlbumSnapshot() {
        val albumTracks = listOf(track("first"), track("second"))
        val detail = AlbumTrackList(album("Double Negative", 2018), window(albumTracks))

        val selection = detail.playbackSelection(1)

        assertEquals(PlaybackSelection(albumTracks, 1), selection)
        assertEquals("second", selection.tracks[selection.startIndex].title)
    }

    @Test
    fun artistSearchShowsAlbumsBeforeArtistsInSeparateSections() {
        showArtistSearch(
            albums = listOf(album("The Great Destroyer", 2005)),
            artists = listOf(LibraryArtist("Low", 4, 2, "content://low")),
        )

        val albumHeading = compose.onNodeWithText("Albums").getUnclippedBoundsInRoot().top
        val artistHeading = compose.onNodeWithText("Artists").getUnclippedBoundsInRoot().top
        assertTrue(albumHeading < artistHeading)
        compose.onNodeWithText("The Great Destroyer").assertIsDisplayed()
        compose.onNodeWithText("Low").assertIsDisplayed()
    }

    @Test
    fun artistSearchAlbumOpensTheAlbumPageDirectly() {
        showArtistSearch(
            albums = listOf(album("The Curtain Hits the Cast", 1996)),
            openedAlbumTracks = listOf(track("Over the Ocean")),
        )

        compose.onNodeWithText("The Curtain Hits the Cast").performClick()

        compose.onNodeWithContentDescription("Back").assertIsDisplayed()
        compose.onNodeWithText("Over the Ocean").assertIsDisplayed()
        compose.onNodeWithContentDescription("Back to artists").assertDoesNotExist()
    }

    @Test
    fun emptyArtistSearchShowsArtistsWithoutAnAlbumSection() {
        showArtistSearch(
            albums = listOf(album("Ones and Sixes", 2015)),
            artists = listOf(LibraryArtist("Low", 4, 2, "content://low")),
            searchText = "",
        )

        compose.onNodeWithText("Low").assertIsDisplayed()
        compose.onNodeWithText("Albums").assertDoesNotExist()
        compose.onNodeWithText("Ones and Sixes").assertDoesNotExist()
    }

    @Test
    fun artistSearchFieldNamesAlbumsAndArtists() {
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                LibrarySearchField(BrowseTab.ARTISTS, "", {}, {})
            }
        }

        compose.onNodeWithText("Search albums and artists").assertIsDisplayed()
    }

    private fun showArtist(
        detail: ArtistTrackList,
        openAlbum: (LibraryAlbum) -> Unit = {},
        openedAlbumTracks: List<LibraryTrack> = emptyList(),
        controls: PlaybackControls = DisconnectedPlaybackControls,
        albumTrackIds: (LibraryAlbum) -> List<Long> = { emptyList() },
    ) {
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                CompositionLocalProvider(
                    LocalPlaybackControls provides controls,
                    LocalAlbumTrackIds provides albumTrackIds,
                ) {
                    var selectedAlbum by remember { mutableStateOf<AlbumTrackList?>(null) }
                    ArtistsTab(
                        surfaceLayout = SurfaceLayout.STACKED,
                        surfaceState = MobileSurfaceViewModel(),
                        artists = LibraryWindow.empty(),
                        searchText = "",
                        selectedArtist = detail,
                        selectedAlbum = selectedAlbum,
                        playback = PlaybackUiState(),
                        openArtist = {},
                        openAlbum = { album ->
                            openAlbum(album)
                            selectedAlbum = AlbumTrackList(album, window(openedAlbumTracks))
                        },
                        closeArtist = {},
                        closeAlbum = { selectedAlbum = null },
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

    private fun showArtistSearch(
        albums: List<LibraryAlbum>,
        artists: List<LibraryArtist> = emptyList(),
        openedAlbumTracks: List<LibraryTrack> = emptyList(),
        searchText: String = "low",
    ) {
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                var selectedAlbum by remember { mutableStateOf<AlbumTrackList?>(null) }
                ArtistsTab(
                    surfaceLayout = SurfaceLayout.STACKED,
                    surfaceState = MobileSurfaceViewModel(),
                    artists = window(artists),
                    albumResults = window(albums),
                    searchText = searchText,
                    selectedArtist = null,
                    selectedAlbum = selectedAlbum,
                    playback = PlaybackUiState(),
                    openArtist = {},
                    openAlbum = { album ->
                        selectedAlbum = AlbumTrackList(album, window(openedAlbumTracks))
                    },
                    closeArtist = {},
                    closeAlbum = { selectedAlbum = null },
                    play = {},
                    lastRequestedOffset = null,
                    artistRequestedOffset = null,
                    loadMoreArtists = {},
                    loadMoreArtistTracks = {},
                )
            }
        }
    }

    private fun artistDetail(
        albums: List<LibraryAlbum> = emptyList(),
        untagged: List<LibraryTrack> = emptyList(),
    ) = ArtistTrackList(
        artist = LibraryArtist("Low", 4, albums.size.toLong(), "content://low"),
        albums = window(albums),
        untaggedTracks = window(untagged),
    )

    private fun album(title: String, year: Int?) = LibraryAlbum(
        title = title,
        artist = "Low",
        representativeUri = "content://$title",
        trackCount = 2,
        year = year,
        totalDurationMs = 120_000,
    )

    private fun track(title: String) = LibraryTrack(
        id = title.hashCode().toLong(),
        uri = "content://$title",
        title = title,
        artist = "Low",
        album = "",
        durationMs = 60_000,
        playCount = 0,
        rating = 0,
    )

    private fun <T> window(rows: List<T>) = LibraryWindow(
        total = rows.size.toLong(),
        rows = rows,
        hasMore = false,
    )
}
