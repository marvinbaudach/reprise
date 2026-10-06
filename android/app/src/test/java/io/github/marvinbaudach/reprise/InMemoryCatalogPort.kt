package io.github.marvinbaudach.reprise

import uniffi.reprise_android_ffi.AndroidArtworkSize

/** The core clamps every read to this many rows, whatever the caller asks for. */
private const val CORE_WINDOW_CAP = 500L

/** One song of the fake catalog; a blank [album] makes it an "other title" of its artist. */
internal data class CatalogSong(
    val id: Long,
    val title: String,
    val artist: String,
    val album: String = "",
)

/**
 * A library the tests can delete from.
 *
 * It answers the same questions the real port does, from a plain list, and it
 * clamps a window at [CORE_WINDOW_CAP] the way `reprise-core` does, so a test
 * that asks for more than the core would give sees `hasMore` and a short page.
 */
internal class InMemoryCatalogPort(songs: List<CatalogSong>) : LibrarySessionPort {
    private var songs = songs

    /** Every window the tests asked for, as `kind:offset:limit`. */
    val reads = mutableListOf<String>()

    fun remove(ids: Collection<Long>) {
        songs = songs.filterNot { it.id in ids }
    }

    fun track(song: CatalogSong) = LibraryTrack(
        id = song.id,
        uri = "content://provider/document/${song.id}.flac",
        title = song.title,
        artist = song.artist,
        album = song.album,
        durationMs = 1_000,
        playCount = 0,
        rating = 0,
    )

    private fun <T> page(kind: String, rows: List<T>, window: LibraryWindowRange): LibraryWindow<T> {
        reads += "$kind:${window.offset}:${window.limit}"
        val offset = window.offset.coerceIn(0, rows.size.toLong()).toInt()
        val limit = window.limit.coerceIn(0, CORE_WINDOW_CAP).toInt()
        val slice = rows.drop(offset).take(limit)
        return LibraryWindow(
            total = rows.size.toLong(),
            rows = slice,
            hasMore = offset + slice.size < rows.size,
        )
    }

    private fun matching(text: String) = songs
        .filter { text.isBlank() || it.title.contains(text, ignoreCase = true) }
        .sortedBy { it.id }
        .map(::track)

    private fun artistRows(): List<LibraryArtist> = songs.groupBy { it.artist }
        .toSortedMap()
        .map { (name, owned) ->
            LibraryArtist(
                name = name,
                trackCount = owned.size.toLong(),
                albumCount = owned.map { it.album }.filter { it.isNotBlank() }.distinct().size.toLong(),
                representativeUri = "content://provider/artist/$name",
            )
        }

    private fun albumRows(artist: String? = null): List<LibraryAlbum> = songs
        .filter { it.album.isNotBlank() && (artist == null || it.artist == artist) }
        .groupBy { it.artist to it.album }
        .toSortedMap(compareBy({ it.first }, { it.second }))
        .map { (key, owned) ->
            LibraryAlbum(
                title = key.second,
                artist = key.first,
                representativeUri = "content://provider/album/${key.second}",
                trackCount = owned.size.toLong(),
                year = null,
                totalDurationMs = owned.size * 1_000L,
            )
        }

    override fun rememberedTreeUri(): String? = "content://provider/tree/Music"
    override fun rememberTreeUri(treeUri: String) = Unit
    override fun persistTreePermission(treeUri: String) = Unit
    override fun isTreeReadable(treeUri: String) = true
    override fun configureTree(treeUri: String) = Unit
    override fun scan(report: (LibraryScreenState.Scanning) -> Unit) = Unit

    override fun searchTracks(text: String, window: LibraryWindowRange) =
        page("titles[$text]", matching(text), window)

    override fun searchAlbums(text: String, window: LibraryWindowRange) =
        page("albums[$text]", albumRows().filter { text.isBlank() || it.title.contains(text, true) }, window)

    override fun listArtists(window: LibraryWindowRange) = page("artists", artistRows(), window)

    override fun searchArtists(text: String, window: LibraryWindowRange) = page(
        "artists[$text]",
        artistRows().filter { it.name.contains(text, ignoreCase = true) },
        window,
    )

    override fun listArtistAlbums(artist: String, window: LibraryWindowRange) =
        page("artist-albums[$artist]", albumRows(artist), window)

    override fun listArtistUntaggedTracks(artist: String, window: LibraryWindowRange) = page(
        "artist-untagged[$artist]",
        songs.filter { it.artist == artist && it.album.isBlank() }.map(::track),
        window,
    )

    override fun listArtistTracks(artist: String, window: LibraryWindowRange) = page(
        "artist-tracks[$artist]",
        songs.filter { it.artist == artist }.map(::track),
        window,
    )

    override fun listAlbumTracks(album: String, albumArtist: String, window: LibraryWindowRange) =
        page(
            "album-tracks[$album]",
            songs.filter { it.album == album && it.artist == albumArtist }.map(::track),
            window,
        )

    override fun albumTrackIds(album: String, albumArtist: String): List<Long> =
        songs.filter { it.album == album && it.artist == albumArtist }.map { it.id }

    override fun artistTrackIds(artist: String): List<Long> =
        songs.filter { it.artist == artist }.map { it.id }

    override fun trackById(trackId: Long): LibraryTrack? =
        songs.firstOrNull { it.id == trackId }?.let(::track)

    override fun artworkFor(trackUri: String, size: AndroidArtworkSize): String? = null
    override fun artistPortraitCached(name: String, size: AndroidArtworkSize): String? = null
    override fun artistPortraitFetched(name: String, size: AndroidArtworkSize): String? = null
    override fun artistsMissingPortraits(limit: UInt): List<String> = emptyList()
    override fun setFavourite(trackId: Long, favourite: Boolean) = Unit
}

/** What the screen holds after paging [rows] titles in, the way scrolling does. */
internal fun InMemoryCatalogPort.pagedIn(text: String, rows: Int): LibraryWindow<LibraryTrack> {
    var window = searchTracks(text, LibraryWindowRange(0, 200))
    while (window.rows.size < rows && window.hasMore) {
        window = window.append(
            searchTracks(text, window.nextRequest(null) ?: break),
        )
    }
    return window
}
