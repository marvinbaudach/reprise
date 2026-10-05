package io.github.marvinbaudach.reprise.library

import uniffi.reprise_android_ffi.AlbumRow
import uniffi.reprise_android_ffi.ArtistRow
import uniffi.reprise_android_ffi.MusicLibrary
import uniffi.reprise_android_ffi.TrackRow
import uniffi.reprise_android_ffi.WindowRange

/** Rows the library hands back per read; it clamps anything larger itself. */
private const val ALBUM_WINDOW = 500

/** The browse tree's reads, answered by the shared native library. */
internal class AndroidMediaBrowseLibrary(private val library: MusicLibrary) : MediaBrowseLibrary {
    override fun recentlyPlayed(limit: Int): List<BrowseTrack> =
        library.recentlyPlayedTracks(limit.toLong()).map(TrackRow::toBrowseTrack)

    override fun playlists(): List<BrowsePlaylist> = library.listPlaylists().map { row ->
        BrowsePlaylist(row.id, row.name, row.trackCount)
    }

    override fun playlistTracks(playlistId: Long): List<BrowseTrack> =
        library.playlistTracks(playlistId).map(TrackRow::toBrowseTrack)

    override fun albums(offset: Int, limit: Int): BrowsePage<BrowseAlbum> {
        val window = library.searchAlbums("", window(offset, limit))
        return BrowsePage(window.rows.map(AlbumRow::toBrowseAlbum), window.hasMore)
    }

    override fun albumTracks(album: String, albumArtist: String): List<BrowseTrack> {
        val tracks = ArrayList<BrowseTrack>()
        while (true) {
            val window = library.listAlbumTracks(
                album,
                albumArtist,
                window(tracks.size, ALBUM_WINDOW),
            )
            tracks += window.rows.map(TrackRow::toBrowseTrack)
            if (!window.hasMore || window.rows.isEmpty()) return tracks
        }
    }

    override fun artists(offset: Int, limit: Int): BrowsePage<BrowseArtist> {
        val window = library.listArtists(window(offset, limit))
        return BrowsePage(window.rows.map(ArtistRow::toBrowseArtist), window.hasMore)
    }

    override fun artistAlbums(artist: String, offset: Int, limit: Int): BrowsePage<BrowseAlbum> {
        val window = library.listArtistAlbums(artist, window(offset, limit))
        return BrowsePage(window.rows.map(AlbumRow::toBrowseAlbum), window.hasMore)
    }

    private fun window(offset: Int, limit: Int) = WindowRange(offset.toLong(), limit.toLong())
}

private fun TrackRow.toBrowseTrack() = BrowseTrack(id, uri, title, artist, album, durationMs)

private fun AlbumRow.toBrowseAlbum() = BrowseAlbum(album, albumArtist, trackCount)

private fun ArtistRow.toBrowseArtist() = BrowseArtist(artist, albumCount, trackCount)
