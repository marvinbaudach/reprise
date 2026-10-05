package io.github.marvinbaudach.reprise.library

/** One track as the browse tree lists it. */
internal data class BrowseTrack(
    val id: Long,
    val uri: String,
    val title: String,
    val artist: String,
    val album: String,
    val durationMs: Long,
)

internal fun BrowseTrack.toTrackMetadata(): TrackMetadata =
    TrackMetadata(id, title, artist, album, durationMs)

internal data class BrowseAlbum(val title: String, val artist: String, val trackCount: Long)

internal data class BrowseArtist(val name: String, val albumCount: Long, val trackCount: Long)

internal data class BrowsePlaylist(val id: Long, val name: String, val trackCount: Long)

/** One window of a larger list. */
internal data class BrowsePage<T>(val rows: List<T>, val hasMore: Boolean)

/**
 * What the media browse tree reads from the library.
 *
 * Kept narrow and free of native types so the tree can be exercised on the JVM
 * with a plain fixture, and so the one place that talks to the UniFFI bindings
 * is [AndroidMediaBrowseLibrary].
 */
internal interface MediaBrowseLibrary {
    /** Present tracks, newest play first. Never more than [limit]. */
    fun recentlyPlayed(limit: Int): List<BrowseTrack>

    fun playlists(): List<BrowsePlaylist>

    /** A playlist's present tracks, in playlist order. */
    fun playlistTracks(playlistId: Long): List<BrowseTrack>

    fun albums(offset: Int, limit: Int): BrowsePage<BrowseAlbum>

    /** Every present track of one album, in disc and track order. */
    fun albumTracks(album: String, albumArtist: String): List<BrowseTrack>

    fun artists(offset: Int, limit: Int): BrowsePage<BrowseArtist>

    fun artistAlbums(artist: String, offset: Int, limit: Int): BrowsePage<BrowseAlbum>
}

/** The words the tree shows for its own folders, resolved from resources. */
internal data class BrowseLabels(
    val root: String,
    val recentlyPlayed: String,
    val playlists: String,
    val albums: String,
    val artists: String,
)
