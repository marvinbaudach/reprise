package io.github.marvinbaudach.reprise

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * What the library hands back after tracks were deleted: the new catalog, and
 * the windows the listener had open rebuilt to the depth they were at.
 */
class LibraryRemovalRefreshTest {
    private val hundreds = (1L..1_200L).map { id ->
        CatalogSong(id, "Song %04d".format(id), artist = "Artist ${id % 3}", album = "Album ${id % 3}")
    }

    private fun windowsOf(
        port: InMemoryCatalogPort,
        titlesLoaded: Int = 200,
        searchText: String = "",
        loadedTabs: Set<BrowseTab> = BrowseTab.entries.toSet(),
        openArtist: ArtistTrackList? = null,
        openAlbum: AlbumTrackList? = null,
    ) = LoadedLibraryWindows(
        titles = port.pagedIn(searchText, titlesLoaded),
        artists = port.listArtists(firstLibraryWindow()),
        loadedTabs = loadedTabs,
        searchText = searchText,
        openAlbum = openAlbum,
        openArtist = openArtist,
    )

    @Test
    fun noPreviousWindowsMeansOnlyTheFreshCatalog() {
        val port = InMemoryCatalogPort(hundreds)
        val refreshed = LibrarySession(port).refreshBrowse(previous = null)

        assertNull(refreshed.windows)
        assertEquals(1_200L, refreshed.state.titles.total)
    }

    @Test
    fun aDeepTitlesWindowIsReadBackToItsPreviousDepthInCoreSizedChunks() {
        val port = InMemoryCatalogPort(hundreds)
        val previous = windowsOf(port, titlesLoaded = 1_000)
        assertEquals(1_000, previous.titles.rows.size)
        port.remove(listOf(3L, 4L, 5L))
        port.reads.clear()

        val refreshed = LibrarySession(port).refreshBrowse(previous)

        val titles = checkNotNull(refreshed.windows).titles
        assertEquals("nothing already loaded may fall away", 1_000, titles.rows.size)
        assertEquals(1_197L, titles.total)
        assertTrue(titles.hasMore)
        assertTrue("a deleted row must be gone", titles.rows.none { it.id in 3L..5L })
        val chunks = port.reads.filter { it.startsWith("titles[]:") }.map { it.substringAfterLast(':').toLong() }
        assertTrue("every read stays within the core cap: $chunks", chunks.all { it <= 500 })
        assertEquals("the state itself keeps the first window", 200, refreshed.state.titles.rows.size)
        assertEquals(1_197L, refreshed.state.titles.total)
    }

    @Test
    fun aWindowShorterThanBeforeIsAskedOnlyForWhatIsLeft() {
        val port = InMemoryCatalogPort(hundreds.take(300))
        val previous = windowsOf(port, titlesLoaded = 300)
        port.remove((1L..100L).toList())

        val refreshed = LibrarySession(port).refreshBrowse(previous)

        val titles = checkNotNull(refreshed.windows).titles
        assertEquals(200, titles.rows.size)
        assertEquals(200L, titles.total)
        assertEquals(false, titles.hasMore)
    }

    @Test
    fun aStandingSearchIsReadAgainAndTheTabsThatNeverHeldItStayEmpty() {
        val port = InMemoryCatalogPort(hundreds)
        val previous = windowsOf(
            port,
            titlesLoaded = 200,
            searchText = "Song 00",
            loadedTabs = setOf(BrowseTab.TITLES),
        )
        port.remove(listOf(1L))

        val refreshed = LibrarySession(port).refreshBrowse(previous)

        val windows = checkNotNull(refreshed.windows)
        assertEquals("Song 00", windows.searchText)
        assertEquals(setOf(BrowseTab.TITLES), windows.loadedTabs)
        assertEquals(98L, windows.titles.total)
        assertTrue(windows.titles.rows.all { it.title.startsWith("Song 00") })
        assertTrue("an unloaded tab must not claim rows", windows.artists.rows.isEmpty())
    }

    @Test
    fun anOpenArtistPageThatShrankStaysOpenWithFreshContent() {
        val port = InMemoryCatalogPort(hundreds.take(30))
        val artist = port.listArtists(firstLibraryWindow()).rows.first { it.name == "Artist 1" }
        val page = LibrarySession(port).openArtist(artist)
        val previous = windowsOf(port, openArtist = page)
        port.remove(listOf(1L, 4L))

        val refreshed = LibrarySession(port).refreshBrowse(previous)

        val open = assertNotNull_(refreshed.windows?.openArtist)
        assertEquals("Artist 1", open.artist.name)
        assertEquals("the header counts are the fresh ones", 8L, open.artist.trackCount)
        assertEquals(1L, open.albums.total)
        assertEquals(8L, open.albums.rows.single().trackCount)
    }

    @Test
    fun anArtistWhoseLastTrackWasDeletedClosesItsPageAndTheAlbumInsideIt() {
        val port = InMemoryCatalogPort(hundreds.take(30))
        val session = LibrarySession(port)
        val artist = port.listArtists(firstLibraryWindow()).rows.first { it.name == "Artist 1" }
        val page = session.openArtist(artist)
        val album = page.albums.rows.single()
        val previous = windowsOf(port, openArtist = page, openAlbum = session.openAlbum(album))
        port.remove(hundreds.take(30).filter { it.artist == "Artist 1" }.map { it.id })

        val refreshed = session.refreshBrowse(previous)

        assertNull(refreshed.windows?.openArtist)
        assertNull(refreshed.windows?.openAlbum)
        assertNotNull(refreshed.windows)
    }

    @Test
    fun anEmptiedAlbumClosesOnlyItselfWhileTheArtistPageStays() {
        val songs = listOf(
            CatalogSong(1, "One", "Solo", "First"),
            CatalogSong(2, "Two", "Solo", "Second"),
            CatalogSong(3, "Three", "Solo", "Second"),
        )
        val port = InMemoryCatalogPort(songs)
        val session = LibrarySession(port)
        val artist = port.listArtists(firstLibraryWindow()).rows.single()
        val page = session.openArtist(artist)
        val first = page.albums.rows.first { it.title == "First" }
        val previous = windowsOf(port, openArtist = page, openAlbum = session.openAlbum(first))
        port.remove(listOf(1L))

        val refreshed = session.refreshBrowse(previous)

        assertNull(refreshed.windows?.openAlbum)
        val open = assertNotNull_(refreshed.windows?.openArtist)
        assertEquals(listOf("Second"), open.albums.rows.map { it.title })
    }

    @Test
    fun anOpenAlbumKeepsFreshTracksAndAFreshTrackCount() {
        val songs = (1L..5L).map { CatalogSong(it, "Track $it", "Solo", "Only") }
        val port = InMemoryCatalogPort(songs)
        val session = LibrarySession(port)
        val artist = port.listArtists(firstLibraryWindow()).rows.single()
        val page = session.openArtist(artist)
        val previous = windowsOf(
            port,
            openArtist = page,
            openAlbum = session.openAlbum(page.albums.rows.single()),
        )
        port.remove(listOf(2L))

        val refreshed = session.refreshBrowse(previous)

        val album = assertNotNull_(refreshed.windows?.openAlbum)
        assertEquals(listOf(1L, 3L, 4L, 5L), album.tracks.rows.map { it.id })
        assertEquals(4L, album.album.trackCount)
    }

    @Test
    fun aFailingRebuildStillDeliversTheFreshCatalogAndSaysWhy() {
        val port = InMemoryCatalogPort(hundreds.take(10))
        val previous = windowsOf(port)
        var titleReads = 0
        val failing = object : LibrarySessionPort by port {
            // The first read is the fresh state's own; the second is the rebuild.
            override fun searchTracks(text: String, window: LibraryWindowRange):
                LibraryWindow<LibraryTrack> {
                titleReads += 1
                check(titleReads < 2) { "provider went away" }
                return port.searchTracks(text, window)
            }
        }

        val refreshed = LibrarySession(failing).refreshBrowse(previous)

        assertNull(refreshed.windows)
        assertNotNull(refreshed.reloadFailure)
    }

    private fun <T : Any> assertNotNull_(value: T?): T {
        assertNotNull(value)
        return value!!
    }
}
