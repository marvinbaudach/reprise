package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import org.junit.After
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.reprise_android_ffi.AndroidColorScheme

/**
 * MTP-66: the tracks a CUE sheet cuts from one file share that file's uri, so a
 * lazy list that keys its rows by uri throws `Key … was already used` the moment
 * the album is on screen. Every list that shows library tracks renders two such
 * tracks here, and every row must come up.
 */
@RunWith(RobolectricTestRunner::class)
@Config(
    sdk = [36],
    qualifiers = "w500dp-h1000dp",
    application = ConfigurationTestApplication::class,
)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class CueRowKeysTest {
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
    fun mtp_66_the_titles_list_shows_both_tracks_of_a_cue_file() {
        showTrackRows(SurfaceLayout.STACKED, LibraryListKey.TITLES)

        assertBothTracksShown()
    }

    @Test
    fun mtp_66_the_wide_short_titles_grid_shows_both_tracks_of_a_cue_file() {
        showTrackRows(SurfaceLayout.WIDE_SHORT, LibraryListKey.TITLES)

        assertBothTracksShown()
    }

    @Test
    fun mtp_66_an_opened_album_shows_both_tracks_of_a_cue_file() {
        showTrackRows(SurfaceLayout.STACKED, LibraryListKey.ALBUM_TRACKS)

        assertBothTracksShown()
    }

    @Test
    fun mtp_66_the_queue_shows_both_tracks_of_a_cue_file() {
        showTrackRows(SurfaceLayout.STACKED, LibraryListKey.UPCOMING, queueActions = noQueueActions)

        assertBothTracksShown()
    }

    @Test
    fun mtp_66_an_artists_other_titles_show_both_tracks_of_a_cue_file() {
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                CompositionLocalProvider(LocalArtistTrackIds provides { emptyList() }) {
                    ArtistsTab(
                        surfaceLayout = SurfaceLayout.STACKED,
                        surfaceState = MobileSurfaceViewModel(),
                        artists = LibraryWindow.empty(),
                        searchText = "",
                        selectedArtist = ArtistTrackList(
                            artist = LibraryArtist("Low", 2, 0, "content://album.opus"),
                            albums = LibraryWindow.empty(),
                            untaggedTracks = cueWindow(),
                        ),
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
        compose.waitForIdle()

        assertBothTracksShown()
    }

    private fun showTrackRows(
        layout: SurfaceLayout,
        listKey: LibraryListKey,
        queueActions: QueueRowActions? = null,
    ) {
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                TrackRows(
                    surfaceLayout = layout,
                    surfaceState = MobileSurfaceViewModel(),
                    listKey = listKey,
                    tracks = cueWindow(),
                    playback = PlaybackUiState().libraryPlayback(),
                    lastRequestedOffset = null,
                    play = {},
                    loadMore = {},
                    queueActions = queueActions,
                )
            }
        }
        compose.waitForIdle()
    }

    private fun assertBothTracksShown() {
        compose.onNodeWithText(FIRST_TITLE).assertIsDisplayed()
        compose.onNodeWithText(SECOND_TITLE).assertIsDisplayed()
    }

    private fun cueWindow() = LibraryWindow(
        total = 2,
        rows = listOf(cueTrack(1, FIRST_TITLE), cueTrack(2, SECOND_TITLE)),
        hasMore = false,
    )

    private fun cueTrack(id: Long, title: String) = LibraryTrack(
        id = id,
        uri = CUE_FILE_URI,
        title = title,
        artist = "Low",
        album = "",
        durationMs = 60_000,
        playCount = 0,
        rating = 0,
    )

    private val noQueueActions = QueueRowActions(
        play = { _, _ -> },
        move = { _, _, _ -> },
        remove = { _, _ -> },
    )

    private companion object {
        const val CUE_FILE_URI = "content://provider/document/album.opus"
        const val FIRST_TITLE = "Opening Movement"
        const val SECOND_TITLE = "Closing Movement"
    }
}
