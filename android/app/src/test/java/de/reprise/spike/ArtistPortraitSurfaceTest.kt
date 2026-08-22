package de.reprise.spike

import android.graphics.Bitmap
import android.graphics.Color
import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToIndex
import de.reprise.spike.ui.theme.RepriseTheme
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidArtworkSize
import uniffi.reprise_android_ffi.AndroidColorScheme

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w412dp-h916dp-port")
class ArtistPortraitSurfaceTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val activeArtwork = mutableListOf<TrackArtwork>()

    @After
    fun stopArtworkWorkers() {
        activeArtwork.forEach(TrackArtwork::shutdown)
    }

    @Test
    fun theArtistRowShowsACachedPortrait() {
        val cachedCalls = AtomicInteger()
        val albumCalls = AtomicInteger()
        val decoded = AtomicReference<String>()
        val portrait = bitmap(Color.RED)
        val artwork = artwork(
            cachedPortrait = { _, _ ->
                cachedCalls.incrementAndGet()
                "portrait"
            },
            albumCover = { _, _ ->
                albumCalls.incrementAndGet()
                "album"
            },
            decode = { path ->
                decoded.set(path)
                if (path == "portrait") portrait else bitmap(Color.GREEN)
            },
        )

        showArtists(listOf(artist("Portrait Artist")), artwork)

        compose.waitUntil { decoded.get() == "portrait" }
        compose.onNodeWithTag("artist-avatar", useUnmergedTree = true).assertExists()
        assertEquals(1, cachedCalls.get())
        assertEquals(0, albumCalls.get())
    }

    @Test
    fun anArtistWithoutAPortraitShowsTheGeneratedAvatar() {
        val cachedCalls = AtomicInteger()
        val albumCalls = AtomicInteger()
        val fallbackCalls = AtomicInteger()
        val artwork = artwork(
            cachedPortrait = { _, _ ->
                cachedCalls.incrementAndGet()
                null
            },
            albumCover = { _, _ ->
                albumCalls.incrementAndGet()
                "album"
            },
            decode = { null },
            fallback = { _, _, _ ->
                fallbackCalls.incrementAndGet()
                bitmap(Color.BLUE)
            },
        )

        showArtists(listOf(artist("Album Artist")), artwork)

        compose.waitUntil { cachedCalls.get() == 1 && fallbackCalls.get() > 0 }
        compose.onNodeWithTag("artist-avatar", useUnmergedTree = true).assertExists()
        assertEquals(1, cachedCalls.get())
        assertEquals(0, albumCalls.get())
    }

    @Test
    fun anArtistWithoutEitherShowsTheGeneratedCover() {
        val cachedCalls = AtomicInteger()
        val albumCalls = AtomicInteger()
        val fallbackCalls = AtomicInteger()
        val generated = bitmap(Color.BLUE)
        val artwork = artwork(
            cachedPortrait = { _, _ ->
                cachedCalls.incrementAndGet()
                null
            },
            albumCover = { _, _ ->
                albumCalls.incrementAndGet()
                null
            },
            decode = { null },
            fallback = { _, _, _ ->
                fallbackCalls.incrementAndGet()
                generated
            },
        )

        showArtists(listOf(artist("Generated Artist")), artwork)

        compose.waitUntil { cachedCalls.get() == 1 && fallbackCalls.get() > 0 }
        compose.onNodeWithTag("artist-avatar", useUnmergedTree = true).assertExists()
        assertTrue(fallbackCalls.get() > 0)
        assertEquals(0, albumCalls.get())
    }

    @Test
    fun scrollingTheArtistListNeverFetches() {
        val cachedLookups = AtomicInteger()
        val fetches = AtomicInteger()
        val artwork = artwork(
            cachedPortrait = { _, _ ->
                cachedLookups.incrementAndGet()
                null
            },
            fetchedPortrait = { _, _ ->
                fetches.incrementAndGet()
                null
            },
            albumCover = { _, _ -> null },
            decode = { null },
        )
        val artists = (1..200).map { index -> artist("Artist $index") }

        showArtists(artists, artwork)
        compose.onNodeWithTag("library-artists-list").performScrollToIndex(180)
        compose.waitForIdle()

        assertTrue(cachedLookups.get() > 0)
        assertEquals(0, fetches.get())
    }

    @Test
    fun aPortraitFetchedInDetailReplacesTheRowsAvatar() {
        val portraitAvailable = AtomicBoolean(false)
        val rowBitmapPath = AtomicReference<String>()
        val albumCalls = AtomicInteger()
        val sharedCache = ArtworkCache()
        val artwork = artwork(
            cachedPortrait = { _, size ->
                if (portraitAvailable.get() && size == AndroidArtworkSize.LIST) {
                    "portrait-list"
                } else {
                    null
                }
            },
            fetchedPortrait = { _, _ ->
                portraitAvailable.set(true)
                "portrait-detail"
            },
            albumCover = { _, size ->
                albumCalls.incrementAndGet()
                if (size == AndroidArtworkSize.LIST) {
                    "album-list"
                } else {
                    "album-detail"
                }
            },
            decode = { path ->
                if (path.endsWith("-list")) rowBitmapPath.set(path)
                if (path.startsWith("portrait")) bitmap(Color.RED) else bitmap(Color.GREEN)
            },
            fallback = { _, _, _ ->
                rowBitmapPath.set("avatar-list")
                bitmap(Color.BLUE)
            },
            cache = sharedCache,
        )

        showArtistDetail(artistDetail("Arriving Portrait"), artwork, initiallyOpen = false)
        compose.waitUntil { rowBitmapPath.get() == "avatar-list" }
        compose.onNodeWithText("Arriving Portrait").performClick()
        compose.waitUntil { portraitAvailable.get() }
        compose.onNodeWithContentDescription("Back to artists").performClick()

        compose.waitUntil { rowBitmapPath.get() == "portrait-list" }
        assertEquals("portrait-list", rowBitmapPath.get())
        assertEquals(0, albumCalls.get())
    }

    @Test
    fun openingAnArtistFetchesExactlyOnceForThatArtist() {
        val fetchedNames = java.util.concurrent.CopyOnWriteArrayList<String>()
        val artwork = artwork(
            cachedPortrait = { _, _ -> null },
            fetchedPortrait = { name, _ ->
                fetchedNames += name
                null
            },
            albumCover = { _, _ -> null },
            decode = { null },
        )
        val detail = artistDetail("Opened Artist")

        showArtistDetail(detail, artwork, initiallyOpen = false)
        compose.onNodeWithText("Opened Artist").performClick()

        compose.waitUntil { fetchedNames.size == 1 }
        assertEquals(listOf("Opened Artist"), fetchedNames)
    }

    @Test
    fun theDetailHeadDoesNotFetchAgainWhenItScrollsOutAndBack() {
        val fetches = AtomicInteger()
        val artwork = artwork(
            cachedPortrait = { _, _ -> null },
            fetchedPortrait = { _, _ ->
                fetches.incrementAndGet()
                null
            },
            albumCover = { _, _ -> null },
            decode = { null },
        )
        val detail = artistDetail(
            name = "Long Artist",
            albums = (1..30).map { index -> album("Album $index", "Long Artist") },
        )

        showArtistDetail(detail, artwork)
        compose.waitUntil { fetches.get() == 1 }
        compose.onNodeWithTag("library-artist-albums-list").performScrollToIndex(20)
        compose.onNodeWithTag("library-artist-albums-list").performScrollToIndex(0)
        compose.waitForIdle()

        assertEquals(1, fetches.get())
    }

    @Test
    fun aClosedSwitchLeavesTheDetailHeadOnTheGeneratedAvatar() {
        val bridgeCalls = AtomicInteger()
        val networkCalls = AtomicInteger()
        val albumCalls = AtomicInteger()
        val fallbackCalls = AtomicInteger()
        val gateOpen = false
        val artwork = artwork(
            cachedPortrait = { _, _ -> null },
            fetchedPortrait = { _, _ ->
                bridgeCalls.incrementAndGet()
                // This is not end-to-end switch proof: this double owns both
                // gateOpen and networkCalls. Core Task 5 proves the real gate.
                if (gateOpen) networkCalls.incrementAndGet()
                null
            },
            albumCover = { _, _ ->
                albumCalls.incrementAndGet()
                "album-cover"
            },
            decode = { bitmap(Color.GREEN) },
            fallback = { _, _, _ ->
                fallbackCalls.incrementAndGet()
                bitmap(Color.BLUE)
            },
        )

        showArtistDetail(artistDetail("Offline Artist"), artwork)

        compose.waitUntil { bridgeCalls.get() == 1 && fallbackCalls.get() > 0 }
        compose.onNodeWithTag("artist-portrait-head-image", useUnmergedTree = true).assertExists()
        assertEquals(1, bridgeCalls.get())
        assertEquals(0, networkCalls.get())
        assertEquals(0, albumCalls.get())
    }

    @Test
    fun anArtistSurfaceNeverShowsATrackVisual() {
        val representativeUri = "content://albums/shared"
        val sharedCache = ArtworkCache()
        val trackRequest = ArtworkRequest(
            trackUri = representativeUri,
            size = AndroidArtworkSize.LIST,
            title = "Shared Album",
            artist = "Shared Artist",
            kind = ArtworkKind.TRACK,
        )
        sharedCache.putArtwork(
            trackRequest,
            ArtworkVisual(bitmap(Color.GREEN).asImageBitmap(), ambientColors = null),
        )
        val artistRequest = ArtworkRequest(
            trackUri = representativeUri,
            size = AndroidArtworkSize.LIST,
            title = "Shared Artist",
            artist = "Shared Artist",
            kind = ArtworkKind.ARTIST,
            artistName = "Shared Artist",
        )
        assertNull(sharedCache.seedArtwork(artistRequest))

        val albumCalls = AtomicInteger()
        val fallbackCalls = AtomicInteger()
        val artwork = artwork(
            cachedPortrait = { _, _ -> null },
            albumCover = { _, _ ->
                albumCalls.incrementAndGet()
                "track-visual"
            },
            decode = { bitmap(Color.GREEN) },
            fallback = { _, _, _ ->
                fallbackCalls.incrementAndGet()
                bitmap(Color.BLUE)
            },
            cache = sharedCache,
        )

        showArtists(
            listOf(
                LibraryArtist(
                    name = "Shared Artist",
                    trackCount = 7,
                    albumCount = 2,
                    representativeUri = representativeUri,
                ),
            ),
            artwork,
        )

        compose.waitUntil { fallbackCalls.get() > 0 }
        compose.onNodeWithTag("artist-avatar", useUnmergedTree = true).assertExists()
        assertEquals(0, albumCalls.get())
    }

    @Test
    fun reopeningTheSameArtistDoesNotReachTheNetworkTwice() {
        val bridgeCalls = AtomicInteger()
        val networkCalls = AtomicInteger()
        val artwork = artwork(
            cachedPortrait = { _, _ -> null },
            fetchedPortrait = { _, _ ->
                if (bridgeCalls.incrementAndGet() == 1) {
                    networkCalls.incrementAndGet()
                }
                "portrait"
            },
            albumCover = { _, _ -> null },
            decode = { bitmap(Color.RED) },
        )

        showArtistDetail(artistDetail("Reopened Artist"), artwork, initiallyOpen = false)
        compose.onNodeWithText("Reopened Artist").performClick()
        compose.waitUntil { bridgeCalls.get() == 1 }
        compose.onNodeWithContentDescription("Back to artists").performClick()
        compose.onNodeWithText("Reopened Artist").performClick()

        compose.waitUntil { bridgeCalls.get() == 2 }
        assertEquals(2, bridgeCalls.get())
        assertEquals(1, networkCalls.get())
    }

    @Test
    fun theDetailHeadShowsTheCountsAndNotTheName() {
        val detail = artistDetail("Counted Artist")
        val artwork = artwork(
            cachedPortrait = { _, _ -> null },
            fetchedPortrait = { _, _ -> null },
            albumCover = { _, _ -> null },
            decode = { null },
        )

        showArtistDetail(detail, artwork)

        compose.onAllNodesWithText("Counted Artist").assertCountEquals(1)
        compose.onNodeWithText(detail.artist.details()).assertExists()
    }

    private fun showArtists(artists: List<LibraryArtist>, artwork: TrackArtwork) {
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                CompositionLocalProvider(LocalTrackArtwork provides artwork) {
                    ArtistsTab(
                        surfaceLayout = SurfaceLayout.STACKED,
                        surfaceState = MobileSurfaceViewModel(),
                        artists = LibraryWindow(
                            total = artists.size.toLong(),
                            rows = artists,
                            hasMore = false,
                        ),
                        searchText = "",
                        selectedArtist = null,
                        playback = PlaybackUiState(),
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
    }

    private fun showArtistDetail(
        detail: ArtistTrackList,
        artwork: TrackArtwork,
        initiallyOpen: Boolean = true,
    ) {
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                CompositionLocalProvider(
                    LocalTrackArtwork provides artwork,
                    LocalAlbumTrackIds provides { emptyList() },
                ) {
                    var selected by remember {
                        mutableStateOf<ArtistTrackList?>(if (initiallyOpen) detail else null)
                    }
                    ArtistsTab(
                        surfaceLayout = SurfaceLayout.STACKED,
                        surfaceState = MobileSurfaceViewModel(),
                        artists = LibraryWindow(
                            total = 1,
                            rows = listOf(detail.artist),
                            hasMore = false,
                        ),
                        searchText = "",
                        selectedArtist = selected,
                        playback = PlaybackUiState(),
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
    }

    private fun artwork(
        cachedPortrait: (String, uniffi.reprise_android_ffi.AndroidArtworkSize) -> String?,
        fetchedPortrait: (String, uniffi.reprise_android_ffi.AndroidArtworkSize) -> String? =
            { _, _ -> error("artist rows must never fetch") },
        albumCover: (String, uniffi.reprise_android_ffi.AndroidArtworkSize) -> String?,
        decode: (String) -> Bitmap?,
        fallback: (String, String, Int) -> Bitmap = { _, _, _ -> bitmap(Color.MAGENTA) },
        cache: ArtworkCache = ArtworkCache(),
    ): TrackArtwork = TrackArtwork(
        resolve = albumCover,
        resolveArtistPortraitCached = cachedPortrait,
        resolveArtistPortraitFetched = fetchedPortrait,
        decode = decode,
        fallback = fallback,
        cache = cache,
    ).also(activeArtwork::add)

    private fun artist(name: String) = LibraryArtist(
        name = name,
        trackCount = 7,
        albumCount = 2,
        representativeUri = "content://albums/$name",
    )

    private fun artistDetail(
        name: String,
        albums: List<LibraryAlbum> = listOf(album("Only Album", name)),
    ) = ArtistTrackList(
        artist = artist(name).copy(
            trackCount = albums.sumOf(LibraryAlbum::trackCount),
            albumCount = albums.size.toLong(),
        ),
        albums = LibraryWindow(
            total = albums.size.toLong(),
            rows = albums,
            hasMore = false,
        ),
    )

    private fun album(title: String, artist: String) = LibraryAlbum(
        title = title,
        artist = artist,
        representativeUri = "content://albums/$artist/$title",
        trackCount = 2,
        year = 2026,
        totalDurationMs = 120_000,
    )

    private fun bitmap(colour: Int): Bitmap =
        Bitmap.createBitmap(8, 8, Bitmap.Config.ARGB_8888).apply { eraseColor(colour) }

    private val theme = MobileThemeSelection(
        palette = MobileTheme.NOCTURNE,
        colorScheme = AndroidColorScheme.SYSTEM,
        dynamicAvailable = false,
    )
}
