package io.github.marvinbaudach.reprise.library

import android.net.Uri
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata

/**
 * The most one parent ever lists, so a browser that cannot page cannot ask for
 * the world and one answer stays well inside a binder transaction.
 */
internal const val MAX_CHILDREN = 2_000

/** Largest window the library answers in one read. */
private const val WINDOW_LIMIT = 500

/** How many songs "Recently played" holds. */
internal const val RECENTLY_PLAYED_LIMIT = 50

/** The songs a play request queues and where in them it starts. */
internal data class BrowseQueue(
    val container: BrowseId,
    val trackIds: List<Long>,
    val startIndex: Int,
)

/**
 * The browse tree Android Auto and other media browsers walk:
 *
 * ```
 * root
 * ├─ Recently played ── songs
 * ├─ Playlists ──────── playlist ── songs
 * ├─ Albums ─────────── album ───── songs
 * └─ Artists ────────── artist ──── album ── songs
 * ```
 *
 * Every read is blocking, so callers run it off the main thread.
 */
internal class MediaBrowseTree(
    private val library: MediaBrowseLibrary,
    private val labels: BrowseLabels,
) {
    fun root(): MediaItem = folder(BrowseId.Root, labels.root, MediaMetadata.MEDIA_TYPE_FOLDER_MIXED)

    /** One node by id; `null` for an id this tree never produced or that no longer exists. */
    fun item(mediaId: String): MediaItem? = when (val id = BrowseId.parse(mediaId)) {
        null -> null
        BrowseId.Root -> root()
        else -> topFolder(id) ?: when (id) {
            is BrowseId.Track -> trackItem(id)
            else -> null
        }
    }

    /**
     * The children of [parentId] for one page, or `null` when the parent is not
     * a browsable node.
     *
     * A parent lists at most [MAX_CHILDREN] rows. A page that starts at or past
     * that row is empty, which is how a paging browser learns the list has
     * ended, and a page that straddles it is cut there. `pageSize` may be
     * [Int.MAX_VALUE] for a browser that does not page (Android Auto's legacy
     * binding asks that way); it then gets the first [MAX_CHILDREN] rows and
     * has no way to ask for more, which is the price of a bounded answer.
     */
    fun children(parentId: String, page: Int, pageSize: Int): List<MediaItem>? {
        val parent = BrowseId.parse(parentId) ?: return null
        val requested = pageSize.coerceIn(0, MAX_CHILDREN)
        val start = page.toLong().coerceAtLeast(0) * requested
        if (requested == 0 || start >= MAX_CHILDREN) return emptyList()
        val offset = start.toInt()
        val limit = minOf(requested, MAX_CHILDREN - offset)
        return when (parent) {
            BrowseId.Root -> listOf(
                BrowseId.RecentlyPlayed,
                BrowseId.Playlists,
                BrowseId.Albums,
                BrowseId.Artists,
            ).mapNotNull(::topFolder).slice(offset, limit)
            BrowseId.RecentlyPlayed,
            is BrowseId.Playlist,
            is BrowseId.Album,
            -> tracks(parent).slice(offset, limit).map { track -> leaf(parent, track) }
            BrowseId.Playlists -> library.playlists().slice(offset, limit).map(::playlistFolder)
            BrowseId.Albums -> windowed(offset, limit, library::albums).map(::albumFolder)
            BrowseId.Artists -> windowed(offset, limit, library::artists).map(::artistFolder)
            is BrowseId.Artist ->
                windowed(offset, limit) { start, count -> library.artistAlbums(parent.name, start, count) }
                    .map(::albumFolder)
            is BrowseId.Track -> null
        }
    }

    /**
     * What tapping a song queues: its whole container, positioned on the song.
     * A song that has dropped out of its container since it was listed plays
     * alone rather than not at all.
     */
    fun queueFor(mediaId: String): BrowseQueue? {
        val leaf = BrowseId.parse(mediaId) as? BrowseId.Track ?: return null
        val ids = tracks(leaf.container).map(BrowseTrack::id)
        val index = ids.indexOf(leaf.trackId)
        return if (index >= 0) {
            BrowseQueue(leaf.container, ids, index)
        } else {
            BrowseQueue(leaf.container, listOf(leaf.trackId), 0)
        }
    }

    private fun topFolder(id: BrowseId): MediaItem? = when (id) {
        BrowseId.RecentlyPlayed -> folder(
            id,
            labels.recentlyPlayed,
            MediaMetadata.MEDIA_TYPE_PLAYLIST,
        )
        BrowseId.Playlists -> folder(id, labels.playlists, MediaMetadata.MEDIA_TYPE_FOLDER_PLAYLISTS)
        BrowseId.Albums -> folder(id, labels.albums, MediaMetadata.MEDIA_TYPE_FOLDER_ALBUMS)
        BrowseId.Artists -> folder(id, labels.artists, MediaMetadata.MEDIA_TYPE_FOLDER_ARTISTS)
        is BrowseId.Playlist -> library.playlists()
            .firstOrNull { it.id == id.playlistId }
            ?.let(::playlistFolder)
        is BrowseId.Album -> albumFolder(BrowseAlbum(id.title, id.artist, trackCount = 0))
        is BrowseId.Artist -> artistFolder(BrowseArtist(id.name, albumCount = 0, trackCount = 0))
        BrowseId.Root, is BrowseId.Track -> null
    }

    private fun trackItem(id: BrowseId.Track): MediaItem? =
        tracks(id.container).firstOrNull { it.id == id.trackId }?.let { leaf(id.container, it) }

    private fun tracks(container: BrowseId): List<BrowseTrack> = when (container) {
        BrowseId.RecentlyPlayed -> library.recentlyPlayed(RECENTLY_PLAYED_LIMIT)
        is BrowseId.Playlist -> library.playlistTracks(container.playlistId)
        is BrowseId.Album -> library.albumTracks(container.title, container.artist)
        else -> emptyList()
    }

    private fun leaf(container: BrowseId, track: BrowseTrack): MediaItem =
        MediaItem.Builder()
            .setMediaId(BrowseId.Track(container, track.id).mediaId)
            .setUri(Uri.parse(track.uri))
            .setMediaMetadata(track.toTrackMetadata().toMediaMetadata())
            .build()

    private fun playlistFolder(playlist: BrowsePlaylist): MediaItem = folder(
        BrowseId.Playlist(playlist.id),
        playlist.name,
        MediaMetadata.MEDIA_TYPE_PLAYLIST,
    )

    private fun albumFolder(album: BrowseAlbum): MediaItem = folder(
        BrowseId.Album(album.title, album.artist),
        album.title,
        MediaMetadata.MEDIA_TYPE_ALBUM,
        subtitle = album.artist,
    )

    private fun artistFolder(artist: BrowseArtist): MediaItem = folder(
        BrowseId.Artist(artist.name),
        artist.name,
        MediaMetadata.MEDIA_TYPE_ARTIST,
    )

    private fun folder(
        id: BrowseId,
        title: String,
        mediaType: Int,
        subtitle: String? = null,
    ): MediaItem = MediaItem.Builder()
        .setMediaId(id.mediaId)
        .setMediaMetadata(
            MediaMetadata.Builder()
                .setTitle(title)
                .setDisplayTitle(title)
                .setSubtitle(subtitle?.ifBlank { null })
                .setArtist(subtitle?.ifBlank { null })
                .setMediaType(mediaType)
                .setIsBrowsable(true)
                .setIsPlayable(false)
                .build(),
        )
        .build()

    /**
     * Reads [limit] rows from [offset] through windows of at most [WINDOW_LIMIT],
     * so a browser that asks for everything is not answered with the library's
     * own page cap.
     */
    private fun <T> windowed(
        offset: Int,
        limit: Int,
        read: (offset: Int, limit: Int) -> BrowsePage<T>,
    ): List<T> {
        val rows = ArrayList<T>()
        var next = offset
        while (rows.size < limit) {
            val page = read(next, minOf(WINDOW_LIMIT, limit - rows.size))
            rows += page.rows
            next += page.rows.size
            if (!page.hasMore || page.rows.isEmpty()) break
        }
        return rows
    }
}

private fun <T> List<T>.slice(offset: Int, limit: Int): List<T> =
    if (offset >= size) emptyList() else subList(offset, minOf(size, offset + limit))
