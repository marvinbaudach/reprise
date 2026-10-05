package io.github.marvinbaudach.reprise.library

import androidx.media3.common.MediaItem
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/**
 * The order the browse tree lists playlists, albums and artists in.
 *
 * The tree decides none of it: playlists come in the listener's own order, albums
 * and artists in the core's order, and the tree hands them on as they are. These
 * fixtures are deliberately against the alphabet, so a tree that sorted, reversed
 * or re-grouped its rows could not pass by accident.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class MediaBrowseTreeOrderTest {
    private val albums = listOf(
        BrowseAlbum("Zenith", "Yankee", trackCount = 1),
        BrowseAlbum("Aurora", "Yankee", trackCount = 1),
        BrowseAlbum("Meridian", "Bravo", trackCount = 1),
    )
    private val library = FixtureBrowseLibrary(
        playlistList = listOf(
            BrowsePlaylist(30, "Zebra", 0),
            BrowsePlaylist(10, "Apple", 0),
            BrowsePlaylist(20, "Mango", 0),
        ),
        albumList = albums,
        artistList = listOf(
            BrowseArtist("Yankee", 2, 2),
            BrowseArtist("Bravo", 1, 1),
            BrowseArtist("Xray", 0, 0),
        ),
        artistAlbumList = mapOf("Yankee" to albums.take(2)),
    )
    private val tree = MediaBrowseTree(library, TEST_LABELS)

    private fun titles(items: List<MediaItem>?) = items!!.map { it.mediaMetadata.title.toString() }

    @Test
    fun playlistsAreListedInTheListenersOwnOrderNotAlphabetically() {
        assertEquals(
            listOf("Zebra", "Apple", "Mango"),
            titles(tree.children(BrowseId.Playlists.mediaId, 0, 50)),
        )
    }

    @Test
    fun albumsAreListedInTheLibrarysOrderNotAlphabetically() {
        assertEquals(
            listOf("Zenith", "Aurora", "Meridian"),
            titles(tree.children(BrowseId.Albums.mediaId, 0, 50)),
        )
    }

    @Test
    fun artistsAreListedInTheLibrarysOrderNotAlphabetically() {
        assertEquals(
            listOf("Yankee", "Bravo", "Xray"),
            titles(tree.children(BrowseId.Artists.mediaId, 0, 50)),
        )
    }

    @Test
    fun anArtistsAlbumsAreListedInTheLibrarysOrderNotAlphabetically() {
        assertEquals(
            listOf("Zenith", "Aurora"),
            titles(tree.children(BrowseId.Artist("Yankee").mediaId, 0, 50)),
        )
    }

    @Test
    fun theOrderSurvivesTheLibrarysWindowsAndTheBrowsersPages() {
        val expected = listOf("Echo", "Alpha", "Delta", "Bravo", "Charlie")
        val named = expected.map { BrowseAlbum(it, "Artist", 1) }
        val windowed = MediaBrowseTree(
            FixtureBrowseLibrary(
                albumList = named,
                artistList = named.map { BrowseArtist(it.title, 1, 1) },
                windowCap = 2,
            ),
            TEST_LABELS,
        )

        // One page that spans three of the library's two-row windows.
        assertEquals(expected, titles(windowed.children(BrowseId.Albums.mediaId, 0, Int.MAX_VALUE)))
        assertEquals(expected, titles(windowed.children(BrowseId.Artists.mediaId, 0, Int.MAX_VALUE)))
        // The same rows page by page, each page starting where the last one stopped.
        val pages = (0..2).flatMap { page -> titles(windowed.children(BrowseId.Albums.mediaId, page, 2)) }
        assertEquals(expected, pages)
    }
}
