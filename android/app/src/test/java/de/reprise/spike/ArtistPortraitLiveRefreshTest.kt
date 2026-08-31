package de.reprise.spike

import android.graphics.Bitmap
import android.graphics.Color
import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performScrollToIndex
import de.reprise.spike.ui.theme.RepriseTheme
import java.util.concurrent.AbstractExecutorService
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
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
class ArtistPortraitLiveRefreshTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun portraitsRebindAsBackfillAdvancesWithoutLosingTheArtistScrollPosition() {
        var portraitAvailable = false
        val cachedReads = AtomicInteger()
        val portrait = bitmap(Color.BLUE)
        val fallback = bitmap(Color.MAGENTA)
        val artwork = TrackArtwork(
            resolve = { _, _ -> null },
            resolveArtistPortraitCached = { _, _ ->
                cachedReads.incrementAndGet()
                if (portraitAvailable) "portrait" else null
            },
            decode = { path -> if (path == "portrait") portrait else null },
            fallback = { _, _, _ -> fallback },
            cache = ArtworkCache(listArtworkCapacity = 64),
            worker = DirectExecutorService(),
            onMainThread = { work -> work() },
        )
        val surfaceState = MobileSurfaceViewModel()
        surfaceState.bindArtistPortraitRefresh(artwork::artistPortraitsChanged)
        surfaceState.acceptArtistPhotoProgress(progress(done = 0))

        try {
            showArtists(surfaceState, artwork)
            compose.onNodeWithTag("library-artists-list").performScrollToIndex(12)
            compose.waitUntil(timeoutMillis = 5_000) {
                surfaceState.scrollPosition(LibraryListKey.ARTISTS).firstVisibleItemIndex >= 12
            }
            val before = surfaceState.scrollPosition(LibraryListKey.ARTISTS)
            compose.waitUntil(timeoutMillis = 5_000) { cachedReads.get() > 0 }
            val readsBeforePortrait = cachedReads.get()

            portraitAvailable = true
            compose.runOnIdle {
                surfaceState.acceptArtistPhotoProgress(progress(done = 1))
            }
            assertEquals(1L, artwork.artistPortraitRevision)
            compose.waitForIdle()
            compose.waitUntil(timeoutMillis = 5_000) {
                cachedReads.get() > readsBeforePortrait
            }
            assertEquals(before, surfaceState.scrollPosition(LibraryListKey.ARTISTS))
            assertTrue(cachedReads.get() > readsBeforePortrait)
        } finally {
            artwork.shutdown()
        }
    }

    private fun showArtists(surfaceState: MobileSurfaceViewModel, artwork: TrackArtwork) {
        val artists = (1..40).map { index ->
            LibraryArtist("Artist $index", 1, 1, "content://artist/$index")
        }
        val browse = LibraryScreenState.Browse(
            titles = LibraryWindow.empty(),
            artists = LibraryWindow(artists.size.toLong(), artists, false),
        )
        surfaceState.initializeSelectedTab(BrowseTab.ARTISTS) {}
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                CompositionLocalProvider(LocalTrackArtwork provides artwork) {
                    BrowseScreen(
                        state = browse,
                        playback = PlaybackUiState().libraryPlayback(),
                        playbackSettingsRevision = 0,
                        surfaceState = surfaceState,
                        chooseFolder = {},
                        rescan = {},
                        searchTitles = { _, _ -> LibraryWindow.empty() },
                        listArtists = { browse.artists },
                        openAlbum = { error("Album navigation is outside this test") },
                        listAlbumTracks = { _, _ -> LibraryWindow.empty() },
                        loadTrack = { _, deliver -> deliver(null) },
                        playTracks = { _, _ -> },
                        loadPlaybackSettings = {
                            PlaybackSettingsUiState(false, true, emptyList())
                        },
                        setEqualizerEnabled = {
                            PlaybackSettingsUiState(it, true, emptyList())
                        },
                        replaceEqualizerCurve = {
                            PlaybackSettingsUiState(false, true, emptyList())
                        },
                        setGaplessEnabled = {
                            PlaybackSettingsUiState(false, it, emptyList())
                        },
                        themeSelection = theme,
                        selectTheme = {},
                        onlineSourcesEnabled = true,
                    )
                }
            }
        }
    }

    private fun progress(done: Long) = ArtistPhotoProgress(
        runId = 1,
        phase = ArtistPhotoProgressPhase.RUNNING,
        done = done,
        failed = 0,
        total = 40,
    )

    private fun bitmap(colour: Int): Bitmap =
        Bitmap.createBitmap(8, 8, Bitmap.Config.ARGB_8888).apply { eraseColor(colour) }

    private val theme = MobileThemeSelection(
        palette = MobileTheme.NOCTURNE,
        colorScheme = AndroidColorScheme.SYSTEM,
        dynamicAvailable = false,
    )
}

private class DirectExecutorService : AbstractExecutorService() {
    private var stopped = false

    override fun execute(command: Runnable) {
        check(!stopped)
        command.run()
    }

    override fun shutdown() {
        stopped = true
    }

    override fun shutdownNow(): List<Runnable> {
        stopped = true
        return emptyList()
    }

    override fun isShutdown(): Boolean = stopped

    override fun isTerminated(): Boolean = stopped

    override fun awaitTermination(timeout: Long, unit: TimeUnit): Boolean = stopped
}
