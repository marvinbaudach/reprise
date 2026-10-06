package io.github.marvinbaudach.reprise

import org.junit.Assert.assertEquals
import org.junit.Test

class LibraryTextTest {
    @Test
    fun countLabelsUseTheSingularOnlyForOne() {
        assertEquals("0 tracks", countLabel(0, "track", "tracks"))
        assertEquals("1 track", countLabel(1, "track", "tracks"))
        assertEquals("2 tracks", countLabel(2, "track", "tracks"))
    }

    @Test
    fun albumAndArtistDetailsUseTheSharedCountLabels() {
        val album = LibraryAlbum("One", "Artist", "content://one", 1, 2026, 58_000)
        val artist = LibraryArtist("Artist", 1, 1, "content://one")

        assertEquals("Artist • 2026 • 1 track", album.details())
        assertEquals("1 album • 1 track", artist.details())
    }
}
