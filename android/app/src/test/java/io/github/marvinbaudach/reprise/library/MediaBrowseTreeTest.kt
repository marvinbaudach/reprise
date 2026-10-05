package io.github.marvinbaudach.reprise.library

import androidx.media3.common.MediaMetadata
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class MediaBrowseTreeTest {
    private val firstAlbum = BrowseAlbum("First Album", "Alpha", trackCount = 2)
    private val secondAlbum = BrowseAlbum("Second Album", "Alpha", trackCount = 1)
    private val library = FixtureBrowseLibrary(
        recently = listOf(browseTrack(3), browseTrack(1)),
        playlistList = listOf(BrowsePlaylist(10, "Road", 2), BrowsePlaylist(11, "Gym", 1)),
        playlistContents = mapOf(10L to listOf(browseTrack(2), browseTrack(3)), 11L to listOf(browseTrack(1))),
        albumList = listOf(firstAlbum, secondAlbum),
        albumContents = mapOf(
            ("First Album" to "Alpha") to listOf(browseTrack(1), browseTrack(2)),
            ("Second Album" to "Alpha") to listOf(browseTrack(3)),
        ),
        artistList = listOf(BrowseArtist("Alpha", 2, 3), BrowseArtist("Beta", 0, 0)),
        artistAlbumList = mapOf("Alpha" to listOf(firstAlbum, secondAlbum)),
    )
    private val tree = MediaBrowseTree(library, TEST_LABELS)

    private fun titles(parent: String, page: Int = 0, size: Int = 50) =
        tree.children(parent, page, size)!!.map { it.mediaMetadata.title.toString() }

    @Test
    fun theRootListsTheFourTopLevelFoldersInOrder() {
        assertEquals(
            listOf("Recently played", "Playlists", "Albums", "Artists"),
            titles(BrowseId.Root.mediaId),
        )
        assertTrue(tree.root().mediaMetadata.isBrowsable == true)
        assertFalse(tree.root().mediaMetadata.isPlayable == true)
    }

    @Test
    fun recentlyPlayedListsPlayableSongsNewestFirst() {
        val songs = tree.children(BrowseId.RecentlyPlayed.mediaId, 0, 50)!!

        assertEquals(listOf("Track 3", "Track 1"), songs.map { it.mediaMetadata.title.toString() })
        assertTrue(songs.all { it.mediaMetadata.isPlayable == true })
        assertTrue(songs.none { it.mediaMetadata.isBrowsable == true })
        assertEquals("content://tree/3.flac", songs[0].localConfiguration?.uri.toString())
        assertEquals("Artist", songs[0].mediaMetadata.artist)
        assertEquals("Album", songs[0].mediaMetadata.albumTitle)
        assertEquals(3_000L, songs[0].mediaMetadata.durationMs)
    }

    @Test
    fun playlistsLeadToTheirSongsInPlaylistOrder() {
        assertEquals(listOf("Road", "Gym"), titles(BrowseId.Playlists.mediaId))

        assertEquals(listOf("Track 2", "Track 3"), titles(BrowseId.Playlist(10).mediaId))
    }

    @Test
    fun albumsLeadToTheirSongsAndNameTheirArtist() {
        val albums = tree.children(BrowseId.Albums.mediaId, 0, 50)!!

        assertEquals(listOf("First Album", "Second Album"), albums.map { it.mediaMetadata.title.toString() })
        assertEquals("Alpha", albums[0].mediaMetadata.subtitle)
        assertEquals(MediaMetadata.MEDIA_TYPE_ALBUM, albums[0].mediaMetadata.mediaType)
        assertEquals(listOf("Track 1", "Track 2"), titles(albums[0].mediaId))
    }

    @Test
    fun artistsLeadToAlbumsAndThenToSongs() {
        assertEquals(listOf("Alpha", "Beta"), titles(BrowseId.Artists.mediaId))

        val albums = tree.children(BrowseId.Artist("Alpha").mediaId, 0, 50)!!
        assertEquals(listOf("First Album", "Second Album"), albums.map { it.mediaMetadata.title.toString() })
        assertEquals(listOf("Track 3"), titles(albums[1].mediaId))
    }

    @Test
    fun anArtistWithoutAlbumsIsAnEmptyFolderNotAnError() {
        assertEquals(emptyList<String>(), titles(BrowseId.Artist("Beta").mediaId))
    }

    @Test
    fun childrenArePagedByPageAndPageSize() {
        val all = (1L..7L).map(::browseTrack)
        val paged = MediaBrowseTree(
            FixtureBrowseLibrary(playlistContents = mapOf(1L to all)),
            TEST_LABELS,
        )
        val playlist = BrowseId.Playlist(1).mediaId

        fun page(number: Int) = paged.children(playlist, number, 3)!!.map { it.mediaMetadata.title.toString() }

        assertEquals(listOf("Track 1", "Track 2", "Track 3"), page(0))
        assertEquals(listOf("Track 4", "Track 5", "Track 6"), page(1))
        assertEquals(listOf("Track 7"), page(2))
        assertEquals(emptyList<String>(), page(3))
    }

    @Test
    fun aBrowserThatDoesNotPageStillGetsEveryAlbumNotJustTheLibrarysWindow() {
        val many = (1..1_300).map { BrowseAlbum("Album $it", "Artist", 1) }
        val big = FixtureBrowseLibrary(albumList = many)
        val paged = MediaBrowseTree(big, TEST_LABELS)

        val everything = paged.children(BrowseId.Albums.mediaId, 0, Int.MAX_VALUE)!!

        assertEquals(1_300, everything.size)
        assertEquals("Album 1300", everything.last().mediaMetadata.title.toString())
        assertEquals(listOf("albums:0:500", "albums:500:500", "albums:1000:500"), big.reads)
    }

    @Test
    fun aPageDeepInTheLibraryReadsOnlyItsOwnWindow() {
        val many = (1..1_300).map { BrowseAlbum("Album $it", "Artist", 1) }
        val big = FixtureBrowseLibrary(albumList = many)

        val page = MediaBrowseTree(big, TEST_LABELS).children(BrowseId.Albums.mediaId, 5, 100)!!

        assertEquals("Album 501", page.first().mediaMetadata.title.toString())
        assertEquals(100, page.size)
        assertEquals(listOf("albums:500:100"), big.reads)
    }

    @Test
    fun aParentThatIsNotAFolderHasNoChildren() {
        assertNull(tree.children(BrowseId.Track(BrowseId.RecentlyPlayed, 1).mediaId, 0, 10))
        assertNull(tree.children("nonsense", 0, 10))
    }

    @Test
    fun itemsAreLookedUpByTheirId() {
        assertEquals("Reprise", tree.item("root")!!.mediaMetadata.title)
        assertEquals("Gym", tree.item(BrowseId.Playlist(11).mediaId)!!.mediaMetadata.title)
        assertNull(tree.item(BrowseId.Playlist(99).mediaId))
        assertNull(tree.item("nonsense"))
        val song = tree.item(BrowseId.Track(BrowseId.Playlist(10), 3).mediaId)
        assertEquals("Track 3", song!!.mediaMetadata.title)
        assertNotNull(song.localConfiguration)
        assertNull(tree.item(BrowseId.Track(BrowseId.Playlist(10), 1).mediaId))
    }

    @Test
    fun tappingASongQueuesItsWholeContainerPositionedOnTheSong() {
        val queue = tree.queueFor(BrowseId.Track(BrowseId.Album("First Album", "Alpha"), 2).mediaId)!!

        assertEquals(listOf(1L, 2L), queue.trackIds)
        assertEquals(1, queue.startIndex)
        assertEquals(BrowseId.Album("First Album", "Alpha"), queue.container)
    }

    @Test
    fun theQueueFollowsTheContainerTheSongWasListedUnder() {
        val fromRecent = tree.queueFor(BrowseId.Track(BrowseId.RecentlyPlayed, 1).mediaId)!!
        val fromPlaylist = tree.queueFor(BrowseId.Track(BrowseId.Playlist(10), 3).mediaId)!!

        assertEquals(listOf(3L, 1L), fromRecent.trackIds)
        assertEquals(1, fromRecent.startIndex)
        assertEquals(listOf(2L, 3L), fromPlaylist.trackIds)
        assertEquals(1, fromPlaylist.startIndex)
    }

    @Test
    fun aSongThatLeftItsContainerPlaysAlone() {
        val queue = tree.queueFor(BrowseId.Track(BrowseId.Playlist(10), 99).mediaId)!!

        assertEquals(listOf(99L), queue.trackIds)
        assertEquals(0, queue.startIndex)
    }

    @Test
    fun onlyASongHasAQueue() {
        assertNull(tree.queueFor(BrowseId.Albums.mediaId))
        assertNull(tree.queueFor("nonsense"))
    }
}
