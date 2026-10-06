package io.github.marvinbaudach.reprise

import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTouchInput
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidRepeatMode
import uniffi.reprise_android_ffi.AndroidTrashFailure
import uniffi.reprise_android_ffi.AndroidTrashReport

/**
 * Deleting from the library screen, end to end: the tracks leave the catalog,
 * the screen re-reads it, and nothing the listener had arranged is lost.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h1000dp")
class DeleteRefreshBehaviorTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val theme = MobileThemeSelection(
        palette = MobileTheme.NOCTURNE,
        colorScheme = uniffi.reprise_android_ffi.AndroidColorScheme.SYSTEM,
        dynamicAvailable = false,
    )

    private val aria = listOf(
        CatalogSong(1, "Aria One", "Aria", "First Light"),
        CatalogSong(2, "Aria Two", "Aria", "First Light"),
        CatalogSong(3, "Aria Three", "Aria", "First Light"),
        CatalogSong(4, "Loose A", "Aria"),
        CatalogSong(5, "Loose B", "Aria"),
        CatalogSong(8, "Wind One", "Aria", "Second Wind"),
    )
    private val bolero = listOf(
        CatalogSong(6, "Bolero One", "Bolero", "Only"),
        CatalogSong(7, "Bolero Two", "Bolero", "Only"),
    )

    /** The screen, its catalog, and the deletion path wired the way the activity wires it. */
    private inner class Screen(songs: List<CatalogSong>) {
        val port = InMemoryCatalogPort(songs)
        val session = LibrarySession(port)
        val timers = ManualTimers()
        val surface = MobileSurfaceViewModel(scheduleAfter = timers::schedule)
        var browse by mutableStateOf(session.refreshBrowse(previous = null).state)
        val controls = DeletionControls(
            remove = port::remove,
            onLibraryChanged = { refresher.refresh() },
        )
        val refresher = LibraryRemovalRefresher(
            session = session,
            surface = surface,
            onWorker = { work -> work() },
            onMain = { work -> work() },
            logFailure = { message, error -> throw AssertionError(message, error) },
        )

        init {
            surface.bindLibraryStateReporter { state -> browse = state as LibraryScreenState.Browse }
        }

        /** Chooses Delete on a title row, lets the undo window pass, and waits for the answer. */
        fun deleteRow(trackId: Long) {
            compose.onNodeWithTag("library-track-row-$trackId").performTouchInput { longClick() }
            compose.onNodeWithText("Delete from device…").performClick()
            passTheUndoWindow()
        }

        fun passTheUndoWindow() {
            compose.waitUntil(5_000) { surface.pendingDeletions.offers.current != null }
            compose.runOnIdle { timers.fireAll() }
            compose.waitForIdle()
        }

        fun show() {
            compose.setContent {
                RepriseTheme(theme, darkPalette = true) {
                    CompositionLocalProvider(
                        LocalPlaybackControls provides controls,
                        LocalDeletionMessages provides surface,
                        LocalAlbumTrackIds provides { album -> session.albumTrackIds(album) },
                        LocalArtistTrackIds provides { artist -> session.artistTrackIds(artist) },
                    ) {
                        BrowseScreen(
                            state = browse,
                            playback = PlaybackUiState().libraryPlayback(),
                            playbackSettingsRevision = 0,
                            surfaceState = surface,
                            chooseFolder = {},
                            rescan = {},
                            themeSelection = theme,
                            selectTheme = {},
                            searchTitles = { text, range -> session.searchTitles(text, range) },
                            listArtists = { range -> session.listArtists(range) },
                            searchArtists = { text, range -> session.searchArtists(text, range) },
                            openAlbum = { album -> session.openAlbum(album) },
                            listAlbumTracks = { album, range -> session.listAlbumTracks(album, range) },
                            openArtist = { artist -> session.openArtist(artist) },
                            listArtistAlbums = { artist, range -> session.listArtistAlbums(artist, range) },
                            listArtistUntaggedTracks = { artist, range ->
                                session.listArtistUntaggedTracks(artist, range)
                            },
                            loadTrack = { _, deliver -> deliver(null) },
                            playTracks = { _, _ -> },
                            loadPlaybackSettings = { PlaybackSettingsUiState(false, true, emptyList()) },
                            setEqualizerEnabled = { PlaybackSettingsUiState(false, true, emptyList()) },
                            replaceEqualizerCurve = { PlaybackSettingsUiState(false, true, emptyList()) },
                            setGaplessEnabled = { PlaybackSettingsUiState(false, true, emptyList()) },
                        )
                    }
                }
            }
        }
    }

    private class DeletionControls(
        private val remove: (List<Long>) -> Unit,
        private val onLibraryChanged: () -> Unit,
    ) : PlaybackControls {
        val requested = mutableListOf<List<Long>>()
        val played = mutableListOf<List<Long>>()

        /** Ids the "provider" refuses to delete; everything else goes. */
        var refused: Set<Long> = emptySet()

        /** While true the outcome is held back, like a slow document provider. */
        var holdOutcome = false
        private var held: (() -> Unit)? = null

        override fun togglePause() = Unit
        override fun next() = Unit
        override fun skipCurrentOrStop() = Unit
        override fun previous() = Unit
        override fun seekTo(positionMs: Long) = Unit
        override fun setShuffle(enabled: Boolean) = Unit
        override fun setRepeat(mode: AndroidRepeatMode) = Unit
        override fun setFavourite(trackId: Long, favourite: Boolean, report: (String?) -> Unit) =
            report(null)

        override fun playTrackIds(trackIds: List<Long>, startIndex: Int) {
            played += trackIds
        }

        override fun deleteTracks(
            trackIds: List<Long>,
            report: (Result<AndroidTrashReport>) -> Unit,
        ) {
            requested += trackIds
            val finish = {
                val gone = trackIds.filterNot { it in refused }
                remove(gone)
                if (gone.isNotEmpty()) onLibraryChanged()
                report(
                    Result.success(
                        AndroidTrashReport(
                            removedIds = gone,
                            failures = trackIds.filter { it in refused }.map {
                                AndroidTrashFailure(it, "content://tracks/$it", "denied")
                            },
                        ),
                    ),
                )
            }
            if (holdOutcome) held = finish else finish()
        }

        fun release() {
            val finish = checkNotNull(held) { "no deletion is waiting" }
            held = null
            finish()
        }
    }

    private fun openArtistPage(name: String) {
        compose.onNodeWithText("Artists").performClick()
        compose.waitForIdle()
        compose.onNodeWithText(name).performClick()
        compose.waitForIdle()
    }

    @Test
    fun aDeepScrollPositionSurvivesADeletionInsideTheLoadedRows() {
        val songs = (1L..700L).map { CatalogSong(it, "Song %04d".format(it), "Artist", "Album") }
        val screen = Screen(songs)
        val first = screen.browse
        screen.surface.keepLoadedWindows(
            first.catalogShape(),
            LoadedLibraryWindows(
                titles = screen.port.pagedIn("", 600),
                artists = first.artists,
                openAlbum = null,
            ),
        )
        screen.surface.updateScroll(LibraryListKey.TITLES, LibraryScrollPosition(450))
        screen.show()
        compose.waitForIdle()
        compose.onNodeWithText("Song 0451").assertIsDisplayed()

        screen.deleteRow(451)

        compose.onNodeWithText("Song 0451").assertDoesNotExist()
        compose.onNodeWithText("Song 0452").assertIsDisplayed()
        assertEquals(450, screen.surface.scrollPosition(LibraryListKey.TITLES).firstVisibleItemIndex)
        val kept = checkNotNull(screen.surface.loadedWindows(screen.browse.catalogShape()))
        assertEquals("the depth paged in before is read back", 600, kept.titles.rows.size)
    }

    @Test
    fun aDeletionFromASearchedListKeepsTheSearchAndTheTab() {
        val screen = Screen(aria + bolero)
        screen.show()
        compose.onNodeWithContentDescription("Search library").performClick()
        compose.onNodeWithText("Search titles").performTextInput("Aria")
        compose.waitForIdle()
        compose.onNodeWithText("Bolero One").assertDoesNotExist()

        screen.deleteRow(2)

        compose.onNodeWithText("Search titles").assertIsDisplayed()
        assertEquals("Aria", screen.surface.searchText)
        assertEquals(BrowseTab.TITLES, screen.surface.selectedTab)
        compose.onNodeWithText("Aria Two").assertDoesNotExist()
        compose.onNodeWithText("Aria One").assertIsDisplayed()
        compose.onNodeWithText("Bolero One").assertDoesNotExist()
    }

    @Test
    fun anArtistPageThatShrankStaysOpenAndShowsWhatIsLeft() {
        val screen = Screen(aria + bolero)
        screen.show()
        openArtistPage("Aria")
        compose.onNodeWithText("Loose A").assertIsDisplayed()

        screen.deleteRow(4)

        compose.onNodeWithText("Loose A").assertDoesNotExist()
        compose.onNodeWithText("Loose B").assertIsDisplayed()
        compose.onNodeWithText("First Light").assertIsDisplayed()
        compose.onNodeWithText("Bolero").assertDoesNotExist()
    }

    @Test
    fun theArtistPagePlayLeavesOutATrackWaitingToBeDeleted() {
        val screen = Screen(aria + bolero)
        screen.show()
        openArtistPage("Aria")
        compose.onNodeWithTag("library-track-row-4").performTouchInput { longClick() }
        compose.onNodeWithText("Delete from device…").performClick()
        compose.waitUntil(5_000) { screen.surface.pendingDeletions.offers.current != null }

        compose.onNodeWithTag("artist-detail-play").performClick()
        compose.waitUntil(5_000) { screen.controls.played.isNotEmpty() }

        assertEquals(listOf(1L, 2L, 3L, 5L, 8L), screen.controls.played.single().sorted())
    }

    @Test
    fun anArtistWhoseEverythingWasDeletedLeavesTheListInPlaceOfTheirPage() {
        val screen = Screen(aria + bolero)
        screen.show()
        openArtistPage("Bolero")
        compose.onNodeWithText("Only").assertIsDisplayed()

        compose.onNodeWithText("Only").performTouchInput { longClick() }
        compose.onNodeWithText("Delete from device…").performClick()
        screen.passTheUndoWindow()

        compose.onNodeWithText("Bolero").assertDoesNotExist()
        compose.onNodeWithText("Aria").assertIsDisplayed()
        assertEquals(listOf(listOf(6L, 7L)), screen.controls.requested)
    }

    @Test
    fun anAlbumEmptiedInsideAnArtistPageLeavesTheArtistPageStanding() {
        val screen = Screen(aria + bolero)
        screen.show()
        openArtistPage("Aria")
        compose.onNodeWithText("Second Wind").performClick()
        compose.waitForIdle()
        compose.onNodeWithText("Wind One").assertIsDisplayed()

        screen.deleteRow(8)

        compose.onNodeWithText("Wind One").assertDoesNotExist()
        compose.onNodeWithText("Second Wind").assertDoesNotExist()
        compose.onNodeWithText("First Light").assertIsDisplayed()
    }

    @Test
    fun theDeletionResultIsShownWhileItRunsAndReplacedByTheOutcome() {
        val screen = Screen(aria + bolero)
        screen.controls.holdOutcome = true
        screen.show()

        screen.deleteRow(6)

        compose.onNodeWithText("Deleting 1 track…").assertIsDisplayed()
        // Hidden since the undo was offered, and not shown again while it runs.
        compose.onNodeWithText("Bolero One").assertDoesNotExist()

        screen.controls.release()
        compose.waitForIdle()

        compose.onNodeWithText("Deleting 1 track…").assertDoesNotExist()
        compose.onNodeWithText("1 track deleted").assertIsDisplayed()
        compose.onNodeWithText("Bolero One").assertDoesNotExist()
    }

    @Test
    fun aDeletionThatMakesItsOwnRowAndPageVanishStillReportsItsOutcome() {
        val screen = Screen(aria + bolero)
        screen.show()
        openArtistPage("Bolero")

        compose.onNodeWithText("Only").performTouchInput { longClick() }
        compose.onNodeWithText("Delete from device…").performClick()
        screen.passTheUndoWindow()

        // The album row, and the whole page around it, are gone; only a
        // message the screen owns can still say what happened.
        compose.onNodeWithText("Only").assertDoesNotExist()
        compose.onNodeWithText("Bolero").assertDoesNotExist()
        compose.onNodeWithText("2 tracks deleted").assertIsDisplayed()
    }

    @Test
    fun aPartialDeletionStillSaysSoAfterTheListReloads() {
        val screen = Screen(aria + bolero)
        screen.controls.refused = setOf(7L)
        screen.show()
        openArtistPage("Bolero")

        compose.onNodeWithText("Only").performTouchInput { longClick() }
        compose.onNodeWithText("Delete from device…").performClick()
        screen.passTheUndoWindow()

        compose.onNodeWithText("1 of 2 could not be deleted").assertIsDisplayed()
        assertEquals(1L, screen.browse.artists.rows.first { it.name == "Bolero" }.trackCount)
    }
}
