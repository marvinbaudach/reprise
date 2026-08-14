package de.reprise.spike

import android.graphics.Bitmap
import android.graphics.Color
import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performScrollToIndex
import de.reprise.spike.ui.theme.RepriseTheme
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
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
    fun anArtistWithoutAPortraitShowsTheAlbumCover() {
        val cachedCalls = AtomicInteger()
        val albumCalls = AtomicInteger()
        val decoded = AtomicReference<String>()
        val album = bitmap(Color.GREEN)
        val artwork = artwork(
            cachedPortrait = { _, _ ->
                cachedCalls.incrementAndGet()
                null
            },
            albumCover = { _, _ ->
                albumCalls.incrementAndGet()
                "album"
            },
            decode = { path ->
                decoded.set(path)
                if (path == "album") album else null
            },
        )

        showArtists(listOf(artist("Album Artist")), artwork)

        compose.waitUntil { decoded.get() == "album" }
        compose.onNodeWithTag("artist-avatar", useUnmergedTree = true).assertExists()
        assertEquals(1, cachedCalls.get())
        assertEquals(1, albumCalls.get())
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

        compose.waitUntil { cachedCalls.get() == 1 && albumCalls.get() == 1 }
        compose.onNodeWithTag("artist-avatar", useUnmergedTree = true).assertExists()
        assertTrue(fallbackCalls.get() > 0)
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

    private fun artwork(
        cachedPortrait: (String, uniffi.reprise_android_ffi.AndroidArtworkSize) -> String?,
        fetchedPortrait: (String, uniffi.reprise_android_ffi.AndroidArtworkSize) -> String? =
            { _, _ -> error("artist rows must never fetch") },
        albumCover: (String, uniffi.reprise_android_ffi.AndroidArtworkSize) -> String?,
        decode: (String) -> Bitmap?,
        fallback: (String, String, Int) -> Bitmap = { _, _, _ -> bitmap(Color.MAGENTA) },
    ): TrackArtwork = TrackArtwork(
        resolve = albumCover,
        resolveArtistPortraitCached = cachedPortrait,
        resolveArtistPortraitFetched = fetchedPortrait,
        decode = decode,
        fallback = fallback,
        cache = ArtworkCache(),
    ).also(activeArtwork::add)

    private fun artist(name: String) = LibraryArtist(
        name = name,
        trackCount = 7,
        albumCount = 2,
        representativeUri = "content://albums/$name",
    )

    private fun bitmap(colour: Int): Bitmap =
        Bitmap.createBitmap(8, 8, Bitmap.Config.ARGB_8888).apply { eraseColor(colour) }

    private val theme = MobileThemeSelection(
        palette = MobileTheme.NOCTURNE,
        colorScheme = AndroidColorScheme.SYSTEM,
        dynamicAvailable = false,
    )
}
