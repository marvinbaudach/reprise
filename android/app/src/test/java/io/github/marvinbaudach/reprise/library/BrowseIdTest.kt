package io.github.marvinbaudach.reprise.library

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class BrowseIdTest {
    @Test
    fun everyKindOfNodeSurvivesAnIdRoundTrip() {
        val album = BrowseId.Album("Rock: Vol. 1 / 100% + more", "Mötley Crüe")
        val ids = listOf(
            BrowseId.Root,
            BrowseId.RecentlyPlayed,
            BrowseId.Playlists,
            BrowseId.Albums,
            BrowseId.Artists,
            BrowseId.Playlist(42),
            album,
            BrowseId.Artist("AC:DC"),
            BrowseId.Track(album, 7, 0),
            BrowseId.Track(BrowseId.RecentlyPlayed, 8, 3),
            BrowseId.Track(BrowseId.Playlist(3), 9, 41),
        )

        ids.forEach { id -> assertEquals(id, BrowseId.parse(id.mediaId)) }
    }

    @Test
    fun aSongKeepsTheContainerItWasListedIn() {
        val album = BrowseId.Album("A", "B")

        val parsed = BrowseId.parse(BrowseId.Track(album, 5, 12).mediaId) as BrowseId.Track

        assertEquals(album, parsed.container)
        assertEquals(5L, parsed.trackId)
        assertEquals(12, parsed.position)
    }

    @Test
    fun anAlbumNameWithTheSeparatorDoesNotShiftTheParts() {
        val tricky = BrowseId.Album("a:b", "c:d")

        assertEquals(tricky, BrowseId.parse(tricky.mediaId))
    }

    @Test
    fun anIdThisTreeNeverProducedParsesToNothing() {
        listOf(
            "",
            "nope",
            "playlist:abc",
            "playlist:1:2",
            "album:only-one-part",
            "artist",
            "root:extra",
            "track:recent",
            "track:recent:notanumber",
            "track:recent:1",
            "track:recent:1:-1",
            "track:recent:1:x",
            "12345",
            "content://tree/1.flac",
        ).forEach { id -> assertNull(id, BrowseId.parse(id)) }
    }

    @Test
    fun aSongCannotBeTheContainerOfAnotherSong() {
        val nested = "track:${BrowseId.Track(BrowseId.RecentlyPlayed, 1, 0).mediaId.replace(":", "%3A")}:2:0"

        assertNull(BrowseId.parse(nested))
    }
}
