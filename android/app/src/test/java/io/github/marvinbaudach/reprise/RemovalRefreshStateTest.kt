package io.github.marvinbaudach.reprise

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * The hand-over of a refreshed library to the screen: the rebuilt windows are
 * put where the screen looks for them, but only while they still describe what
 * the listener is looking at.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class RemovalRefreshStateTest {
    private val songs = (1L..40L).map { CatalogSong(it, "Song $it", "Artist ${it % 2}", "Album ${it % 2}") }

    private class Screen {
        val surface = MobileSurfaceViewModel()
        val delivered = mutableListOf<LibraryScreenState>()

        init {
            surface.bindLibraryStateReporter { delivered += it }
        }
    }

    private fun setUp(): Triple<Screen, InMemoryCatalogPort, LibrarySession> {
        val port = InMemoryCatalogPort(songs)
        return Triple(Screen(), port, LibrarySession(port))
    }

    @Test
    fun theRebuiltWindowsAreWhatTheScreenRestoresFromWhenNothingChangedMeanwhile() {
        val (screen, port, session) = setUp()
        val first = session.refreshBrowse(previous = null).state
        val deep = LoadedLibraryWindows(
            titles = port.pagedIn("", 40),
            artists = first.artists,
            openAlbum = null,
        )
        screen.surface.keepLoadedWindows(first.catalogShape(), deep)
        val basis = screen.surface.removalRefreshBasis()
        port.remove(listOf(1L))

        val refreshed = session.refreshBrowse(basis.windows)
        screen.surface.updateLibraryAfterRemoval(refreshed, basis, screen.surface.takeRefreshTicket())

        val state = screen.delivered.single() as LibraryScreenState.Browse
        assertEquals(39L, state.titles.total)
        val restored = checkNotNull(screen.surface.loadedWindows(state.catalogShape()))
        assertEquals(39, restored.titles.rows.size)
        assertSame(refreshed.windows, restored)
    }

    @Test
    fun aSearchTypedMeanwhileLeavesTheOldWindowsBehind() {
        val (screen, port, session) = setUp()
        val first = session.refreshBrowse(previous = null).state
        screen.surface.keepLoadedWindows(
            first.catalogShape(),
            LoadedLibraryWindows(first.titles, first.artists, openAlbum = null),
        )
        val basis = screen.surface.removalRefreshBasis()
        port.remove(listOf(1L))
        val refreshed = session.refreshBrowse(basis.windows)

        screen.surface.updateSearch("song 2")
        screen.surface.updateLibraryAfterRemoval(refreshed, basis, screen.surface.takeRefreshTicket())

        val state = screen.delivered.single() as LibraryScreenState.Browse
        assertNull(screen.surface.loadedWindows(state.catalogShape()))
    }

    @Test
    fun anArtistPageClosedMeanwhileIsNotPushedBackOpen() {
        val (screen, port, session) = setUp()
        val first = session.refreshBrowse(previous = null).state
        val artist = first.artists.rows.first()
        val page = session.openArtist(artist)
        screen.surface.keepLoadedWindows(
            first.catalogShape(),
            LoadedLibraryWindows(first.titles, first.artists, openAlbum = null, openArtist = page),
        )
        val basis = screen.surface.removalRefreshBasis()
        port.remove(listOf(2L))
        val refreshed = session.refreshBrowse(basis.windows)

        // The listener backed out of the page while the worker was reading.
        screen.surface.keepLoadedWindows(
            first.catalogShape(),
            LoadedLibraryWindows(first.titles, first.artists, openAlbum = null, openArtist = null),
        )
        screen.surface.updateLibraryAfterRemoval(refreshed, basis, screen.surface.takeRefreshTicket())

        val state = screen.delivered.single() as LibraryScreenState.Browse
        assertNull(screen.surface.loadedWindows(state.catalogShape()))
    }

    @Test
    fun aTabSwitchedMeanwhileLeavesTheOldWindowsBehind() {
        val (screen, port, session) = setUp()
        val first = session.refreshBrowse(previous = null).state
        screen.surface.keepLoadedWindows(
            first.catalogShape(),
            LoadedLibraryWindows(first.titles, first.artists, openAlbum = null),
        )
        val basis = screen.surface.removalRefreshBasis()
        port.remove(listOf(1L))
        val refreshed = session.refreshBrowse(basis.windows)

        screen.surface.selectTab(BrowseTab.ARTISTS)
        screen.surface.updateLibraryAfterRemoval(refreshed, basis, screen.surface.takeRefreshTicket())

        val state = screen.delivered.single() as LibraryScreenState.Browse
        assertNull(screen.surface.loadedWindows(state.catalogShape()))
    }

    @Test
    fun aLibraryWithNothingOnScreenYetJustGetsTheFreshState() {
        val (screen, port, session) = setUp()
        val basis = screen.surface.removalRefreshBasis()
        port.remove(listOf(1L))

        screen.surface.updateLibraryAfterRemoval(
            session.refreshBrowse(basis.windows),
            basis,
            screen.surface.takeRefreshTicket(),
        )

        assertEquals(1, screen.delivered.size)
        assertNull(basis.windows)
    }

    @Test
    fun aRefreshThatFinishesAfterANewerOneIsDropped() {
        val (screen, port, session) = setUp()
        val mainQueue = mutableListOf<() -> Unit>()
        val refresher = LibraryRemovalRefresher(
            session = session,
            surface = screen.surface,
            onWorker = { work -> work() },
            onMain = { work -> mainQueue += work },
            logFailure = { message, error -> throw AssertionError(message, error) },
        )

        refresher.refresh()
        port.remove(listOf(1L))
        refresher.refresh()
        mainQueue.reversed().forEach { it() }

        val state = screen.delivered.single() as LibraryScreenState.Browse
        assertEquals("only the newer read is shown", 39L, state.titles.total)
    }

    @Test
    fun aRefreshThatCannotReadLogsAndLeavesTheScreenAlone() {
        val (screen, port, _) = setUp()
        val broken = LibrarySession(object : LibrarySessionPort by port {
            override fun searchTracks(text: String, window: LibraryWindowRange):
                LibraryWindow<LibraryTrack> = error("provider went away")
        })
        val logged = mutableListOf<String>()
        val refresher = LibraryRemovalRefresher(
            session = broken,
            surface = screen.surface,
            onWorker = { work -> work() },
            onMain = { work -> work() },
            logFailure = { message, _ -> logged += message },
        )

        refresher.refresh()

        assertEquals(emptyList<LibraryScreenState>(), screen.delivered)
        assertEquals(1, logged.size)
    }

    @Test
    fun aRefreshStartedByARecreatedActivityOutranksOneFromTheOldActivity() {
        val (screen, port, session) = setUp()
        val oldMain = mutableListOf<() -> Unit>()
        fun refresher(main: MutableList<() -> Unit>) = LibraryRemovalRefresher(
            session = session,
            surface = screen.surface,
            onWorker = { work -> work() },
            onMain = { work -> main += work },
            logFailure = { message, error -> throw AssertionError(message, error) },
        )

        refresher(oldMain).refresh()
        port.remove(listOf(1L))
        // A rotation replaced the activity, and with it the refresher; the
        // view model, which outlives both, is the only thing they share.
        val newMain = mutableListOf<() -> Unit>()
        refresher(newMain).refresh()
        newMain.forEach { it() }
        oldMain.forEach { it() }

        val state = screen.delivered.single() as LibraryScreenState.Browse
        assertEquals(39L, state.titles.total)
    }

    @Test
    fun aListPagedFurtherWhileTheReadRanIsReadAgainToItsNewDepth() {
        val songs = (1L..1_200L).map { CatalogSong(it, "Song %04d".format(it), "Artist", "Album") }
        val port = InMemoryCatalogPort(songs)
        val session = LibrarySession(port)
        val screen = Screen()
        val first = session.refreshBrowse(previous = null).state
        fun keep(rows: Int) = screen.surface.keepLoadedWindows(
            first.catalogShape(),
            LoadedLibraryWindows(port.pagedIn("", rows), first.artists, openAlbum = null),
        )
        keep(200)
        val main = mutableListOf<() -> Unit>()
        val refresher = LibraryRemovalRefresher(
            session = session,
            surface = screen.surface,
            onWorker = { work -> work() },
            onMain = { work -> main += work },
            logFailure = { message, error -> throw AssertionError(message, error) },
        )

        refresher.refresh()
        // The listener keeps scrolling while the worker reads.
        keep(600)
        port.remove(listOf(1L))
        while (main.isNotEmpty()) main.removeAt(0)()

        val state = screen.delivered.single() as LibraryScreenState.Browse
        val restored = checkNotNull(screen.surface.loadedWindows(state.catalogShape()))
        assertEquals("the anchor's rows are all still there", 600, restored.titles.rows.size)
        assertTrue("and the deleted row is not among them", restored.titles.rows.none { it.id == 1L })
    }

    @Test
    fun theDeletionResultOutlivesAnythingTheListDoes() {
        val surface = MobileSurfaceViewModel()

        val run = surface.begin("Deleting 3 tracks…")
        assertEquals("Deleting 3 tracks…", surface.deletionProgress?.text)
        assertNull(surface.deletionMessage)

        run.finish("1 of 3 could not be deleted")
        assertNull("the result replaces the progress", surface.deletionProgress)
        assertEquals("1 of 3 could not be deleted", surface.deletionMessage?.text)

        surface.dismissDeletionMessage()
        assertNull(surface.deletionMessage)
    }

    @Test
    fun aSecondResultWithTheSameTextIsANewEvent() {
        val surface = MobileSurfaceViewModel()

        surface.begin("Deleting 1 track…").finish("1 track deleted")
        val first = checkNotNull(surface.deletionMessage)
        surface.begin("Deleting 1 track…").finish("1 track deleted")

        assertEquals(first.occurrence + 1, surface.deletionMessage?.occurrence)
    }

    @Test
    fun theFirstOverlappingDeletionToAnswerLeavesTheOthersProgressUp() {
        val surface = MobileSurfaceViewModel()

        val slow = surface.begin("Deleting 500 tracks…")
        val quick = surface.begin("Deleting 1 track…")
        assertEquals("the latest start is the one shown", "Deleting 1 track…", surface.deletionProgress?.text)

        slow.finish("500 tracks deleted")
        assertEquals("the quick one is still running", "Deleting 1 track…", surface.deletionProgress?.text)
        assertEquals("500 tracks deleted", surface.deletionMessage?.text)

        quick.finish("1 track deleted")
        assertNull(surface.deletionProgress)
        assertEquals("1 track deleted", surface.deletionMessage?.text)
    }

    @Test
    fun aRunThatAnswersTwiceEndsOnlyItself() {
        val surface = MobileSurfaceViewModel()

        val first = surface.begin("Deleting 2 tracks…")
        surface.begin("Deleting 1 track…")
        first.finish("2 tracks deleted")
        first.finish("2 tracks deleted")

        assertEquals("Deleting 1 track…", surface.deletionProgress?.text)
    }
}
