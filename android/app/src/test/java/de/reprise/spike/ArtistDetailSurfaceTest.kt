package de.reprise.spike

import android.graphics.Bitmap
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PixelMap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.captureToImage
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
import androidx.compose.ui.unit.dp
import de.reprise.spike.ui.theme.RepriseTheme
import org.junit.After
import org.junit.Rule
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.reprise_android_ffi.AndroidColorScheme

@RunWith(RobolectricTestRunner::class)
@Config(
    sdk = [36],
    qualifiers = "w500dp-h1000dp",
    application = ConfigurationTestApplication::class,
)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ArtistDetailSurfaceTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val theme = MobileThemeSelection(
        palette = MobileTheme.NOCTURNE,
        colorScheme = AndroidColorScheme.SYSTEM,
        dynamicAvailable = false,
    )
    private val application: ConfigurationTestApplication
        get() = RuntimeEnvironment.getApplication() as ConfigurationTestApplication

    @After
    fun releaseTheService() {
        application.releaseService()
    }

    @Test
    fun artistPortraitShimmerTurnsAQuarterTurn() {
        val (before, after) = shimmerFrames(animationsEnabled = true)

        assertTrue(
            "a quarter turn changed no off-axis shimmer pixels",
            before.offAxisDifference(after) > 40,
        )
    }

    @Test
    fun artistPortraitShimmerStopsWhenAnimationsAreDisabled() {
        val (before, after) = shimmerFrames(animationsEnabled = false)

        assertEquals(0, before.offAxisDifference(after))
    }

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
        compose.onNodeWithContentDescription("Back").performClick()
        compose.onNodeWithContentDescription("Back to artists").assertIsDisplayed()
    }

    @Test
    fun artistWithUntaggedTracksShowsOtherTitles() {
        showArtist(artistDetail(untagged = listOf(track("Laser Beam"))))
        compose.onNodeWithText("Other titles").assertIsDisplayed()
        compose.onNodeWithText("Laser Beam").assertIsDisplayed()
        compose.onNodeWithText("Unknown album").assertDoesNotExist()
    }

    @Test
    fun albumOnlyRowWithAlbumShowsItsAlbumName() {
        showArtist(
            artistDetail(
                untagged = listOf(track("Dinosaur Act", album = "Things We Lost in the Fire")),
            ),
        )

        compose.onNodeWithText("Dinosaur Act").assertIsDisplayed()
        compose.onNodeWithText("Things We Lost in the Fire").assertIsDisplayed()
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
    fun nonBlankArtistSearchShowsArtistsWithoutAnAlbumSection() {
        showArtistSearch(
            artists = listOf(LibraryArtist("Low", 4, 2, "content://low")),
        )

        compose.onNodeWithText("Low").assertIsDisplayed()
        compose.onNodeWithText("Albums").assertDoesNotExist()
    }

    @Test
    fun emptyArtistSearchShowsArtistsWithoutAnAlbumSection() {
        showArtistSearch(
            artists = listOf(LibraryArtist("Low", 4, 2, "content://low")),
            searchText = "",
        )

        compose.onNodeWithText("Low").assertIsDisplayed()
        compose.onNodeWithText("Albums").assertDoesNotExist()
    }

    @Test
    fun artistSearchFieldNamesArtists() {
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                LibrarySearchField(BrowseTab.ARTISTS, "", {}, {})
            }
        }

        compose.onNodeWithText("Search artists").assertIsDisplayed()
    }

    private fun shimmerFrames(animationsEnabled: Boolean): Pair<PixelMap, PixelMap> {
        application.animationsEnabled = animationsEnabled
        val controller = AmbientMotionController()
        val bitmap = asymmetricArtwork()
        val image = bitmap.asImageBitmap()
        SharedArtworkCache.putFog(
            image,
            prepareCoverFogBitmap(bitmap, android.graphics.Color.DKGRAY),
        )
        val visual = ArtworkVisual(image, ambientColors = null)
        compose.mainClock.autoAdvance = false
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                CompositionLocalProvider(LocalAmbientMotionController provides controller) {
                    BindAmbientRuntime(controller) { application.animationsEnabled }
                    Box(
                        Modifier
                            .size(width = 300.dp, height = 500.dp)
                            .background(Color.Black),
                    ) {
                        ArtistPortraitShimmer(
                            visual = visual,
                            playing = false,
                            coverDiameterDp = 80f,
                            centerFraction = SHIMMER_CENTER_FRACTION,
                            modifier = Modifier
                                .fillMaxSize()
                                .testTag("artist-portrait-shimmer"),
                        )
                    }
                }
            }
        }
        compose.mainClock.advanceTimeBy(FRAME_MS)
        val before = compose.onNodeWithTag("artist-portrait-shimmer").captureToImage().toPixelMap()
        compose.mainClock.advanceTimeBy(QUARTER_TURN_MS)
        val after = compose.onNodeWithTag("artist-portrait-shimmer").captureToImage().toPixelMap()
        return before to after
    }

    private fun PixelMap.offAxisDifference(other: PixelMap): Int {
        val centerY = (height * SHIMMER_CENTER_FRACTION).toInt()
        val top = (centerY - height / 5).coerceAtLeast(0)
        return (top until centerY).sumOf { y ->
            (width / 10 until width * 2 / 5).count { x -> this[x, y] != other[x, y] }
        }
    }

    private fun asymmetricArtwork(): Bitmap = Bitmap.createBitmap(
        64,
        64,
        Bitmap.Config.ARGB_8888,
    ).apply {
        for (y in 0 until height) {
            for (x in 0 until width) {
                val colour = when {
                    x < width / 2 && y < height / 2 -> android.graphics.Color.RED
                    x >= width / 2 && y < height / 2 -> android.graphics.Color.GREEN
                    x < width / 2 -> android.graphics.Color.BLUE
                    else -> android.graphics.Color.YELLOW
                }
                setPixel(x, y, colour)
            }
        }
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
                        playback = PlaybackUiState().libraryPlayback(),
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
        artists: List<LibraryArtist> = emptyList(),
        searchText: String = "low",
    ) {
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                ArtistsTab(
                    surfaceLayout = SurfaceLayout.STACKED,
                    surfaceState = MobileSurfaceViewModel(),
                    artists = window(artists),
                    searchText = searchText,
                    selectedArtist = null,
                    playback = PlaybackUiState().libraryPlayback(),
                    openArtist = {},
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

    private fun track(title: String, album: String = "") = LibraryTrack(
        id = title.hashCode().toLong(),
        uri = "content://$title",
        title = title,
        artist = "Low",
        album = album,
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

private const val FRAME_MS = 16L
private const val QUARTER_TURN_MS = 15_000L
private const val SHIMMER_CENTER_FRACTION = 0.35f
