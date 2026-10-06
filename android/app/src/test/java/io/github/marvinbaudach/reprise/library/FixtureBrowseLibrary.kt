package io.github.marvinbaudach.reprise.library

/** A small in-memory library for the browse tree. */
internal class FixtureBrowseLibrary(
    val recently: List<BrowseTrack> = emptyList(),
    val playlistList: List<BrowsePlaylist> = emptyList(),
    val playlistContents: Map<Long, List<BrowseTrack>> = emptyMap(),
    val albumList: List<BrowseAlbum> = emptyList(),
    val albumContents: Map<Pair<String, String>, List<BrowseTrack>> = emptyMap(),
    val artistList: List<BrowseArtist> = emptyList(),
    val artistAlbumList: Map<String, List<BrowseAlbum>> = emptyMap(),
    /** The most rows one read returns, like the core's own window cap. */
    private val windowCap: Int = 500,
) : MediaBrowseLibrary {
    /** Every windowed read as `kind:offset:limit`, as the library saw it. */
    val reads = mutableListOf<String>()

    override fun recentlyPlayed(limit: Int) = recently.take(limit)

    override fun playlists() = playlistList

    override fun playlistTracks(playlistId: Long) = playlistContents[playlistId].orEmpty()

    override fun albums(offset: Int, limit: Int) = window("albums", albumList, offset, limit)

    override fun albumTracks(album: String, albumArtist: String) =
        albumContents[album to albumArtist].orEmpty()

    override fun artists(offset: Int, limit: Int) = window("artists", artistList, offset, limit)

    override fun artistAlbums(artist: String, offset: Int, limit: Int) =
        window("artist:$artist", artistAlbumList[artist].orEmpty(), offset, limit)

    private fun <T> window(kind: String, rows: List<T>, offset: Int, limit: Int): BrowsePage<T> {
        reads += "$kind:$offset:$limit"
        val end = minOf(rows.size, offset + minOf(limit, windowCap))
        val slice = if (offset >= rows.size) emptyList() else rows.subList(offset, end)
        return BrowsePage(slice, end < rows.size)
    }
}

internal fun browseTrack(id: Long, title: String = "Track $id") =
    BrowseTrack(id, "content://tree/$id.flac", title, "Artist", "Album", 1_000L * id)

internal val TEST_LABELS = BrowseLabels(
    root = "Reprise",
    recentlyPlayed = "Recently played",
    playlists = "Playlists",
    albums = "Albums",
    artists = "Artists",
)
