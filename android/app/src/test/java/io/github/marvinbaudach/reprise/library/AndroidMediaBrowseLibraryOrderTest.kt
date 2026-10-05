package io.github.marvinbaudach.reprise.library

import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AlbumRow
import uniffi.reprise_android_ffi.AlbumWindow
import uniffi.reprise_android_ffi.ArtistRow
import uniffi.reprise_android_ffi.ArtistWindow
import uniffi.reprise_android_ffi.MusicLibrary
import uniffi.reprise_android_ffi.NoHandle
import uniffi.reprise_android_ffi.PlaylistRow
import uniffi.reprise_android_ffi.WindowRange

/** The core answers in two-row windows here, so every list is read in several. */
private const val CORE_WINDOW = 2

/**
 * A core that lists its rows in a fixed order, against the alphabet, in small
 * windows. Whatever the order is, it is the core's: the app must not change it.
 */
private class OrderedCore : MusicLibrary(NoHandle) {
    val playlists = listOf(PlaylistRow(3, "Zebra", 0), PlaylistRow(1, "Apple", 0), PlaylistRow(2, "Mango", 0))
    val albums = listOf("Echo", "Delta", "Charlie", "Bravo", "Alpha").map(::album)
    val artists = listOf("Yankee", "Bravo", "Xray", "Alice", "Zed").map { name -> ArtistRow(name, 1, 1, "") }

    override fun listPlaylists(): List<PlaylistRow> = playlists

    override fun searchAlbums(text: String, window: WindowRange): AlbumWindow {
        val rows = windowOf(albums, window)
        return AlbumWindow(albums.size.toLong(), rows, window.offset + rows.size < albums.size)
    }

    override fun listArtists(window: WindowRange): ArtistWindow {
        val rows = windowOf(artists, window)
        return ArtistWindow(artists.size.toLong(), rows, window.offset + rows.size < artists.size)
    }

    override fun listArtistAlbums(artist: String, window: WindowRange): AlbumWindow {
        val rows = windowOf(albums, window)
        return AlbumWindow(albums.size.toLong(), rows, window.offset + rows.size < albums.size)
    }

    private fun album(title: String) = AlbumRow(title, "Artist", "", 1, null, 1_000)

    private fun <T> windowOf(all: List<T>, window: WindowRange): List<T> =
        all.drop(window.offset.toInt()).take(minOf(window.limit.toInt(), CORE_WINDOW))
}

/** The Android Auto tree built on the real adapter keeps the core's order, across the core's windows. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class AndroidMediaBrowseLibraryOrderTest {
    private val core = OrderedCore()
    private val tree = MediaBrowseTree(AndroidMediaBrowseLibrary(core), TEST_LABELS)

    private fun titles(parent: BrowseId) =
        tree.children(parent.mediaId, 0, Int.MAX_VALUE)!!.map { it.mediaMetadata.title.toString() }

    @Test
    fun playlistsKeepTheCoresOrder() {
        assertEquals(core.playlists.map { it.name }, titles(BrowseId.Playlists))
    }

    @Test
    fun albumsKeepTheCoresOrderAcrossItsWindows() {
        assertEquals(core.albums.map { it.album }, titles(BrowseId.Albums))
    }

    @Test
    fun artistsKeepTheCoresOrderAcrossItsWindows() {
        assertEquals(core.artists.map { it.artist }, titles(BrowseId.Artists))
    }

    @Test
    fun anArtistsAlbumsKeepTheCoresOrderAcrossItsWindows() {
        assertEquals(core.albums.map { it.album }, titles(BrowseId.Artist("Yankee")))
    }
}
