package de.reprise.spike

private const val LIBRARY_WINDOW_SIZE = 200L

internal data class LibraryWindowRange(
    val offset: Long,
    val limit: Long,
)

internal data class LibraryWindow<T>(
    val total: Long,
    val rows: List<T>,
    val hasMore: Boolean,
) {
    fun append(continuation: LibraryWindow<T>): LibraryWindow<T> = LibraryWindow(
        total = continuation.total,
        rows = rows + continuation.rows,
        hasMore = continuation.hasMore,
    )

    fun nextRequest(lastRequestedOffset: Long?): LibraryWindowRange? {
        val offset = rows.size.toLong()
        if (!hasMore || offset >= total || offset == lastRequestedOffset) {
            return null
        }
        return LibraryWindowRange(
            offset = offset,
            limit = minOf(LIBRARY_WINDOW_SIZE, total - offset),
        )
    }

    /**
     * How many items a lazy list built from this window holds — the loaded rows
     * plus the continuation sentinel, when there is one. It is what an anchor
     * has to be checked against: the sentinel is a place the listener can be
     * looking at.
     */
    fun itemCount(lastRequestedOffset: Long?): Int =
        rows.size + if (nextRequest(lastRequestedOffset) == null) 0 else 1

    companion object {
        fun <T> empty(): LibraryWindow<T> = LibraryWindow(
            total = 0,
            rows = emptyList(),
            hasMore = false,
        )
    }
}

internal data class LibraryWindowRemoval<T>(
    val window: LibraryWindow<T>,
    val lastRequestedOffset: Long?,
)

/** Removes a loaded favourite and invalidates paging state only when rows shrink. */
internal fun LibraryWindow<LibraryTrack>.removeTrack(
    trackId: Long,
    lastRequestedOffset: Long?,
): LibraryWindowRemoval<LibraryTrack> {
    val remaining = rows.filterNot { it.id == trackId }
    val removed = rows.size - remaining.size
    return LibraryWindowRemoval(
        window = copy(
            total = (total - removed).coerceAtLeast(0),
            rows = remaining,
        ),
        lastRequestedOffset = if (removed == 0) lastRequestedOffset else null,
    )
}

internal fun firstLibraryWindow() = LibraryWindowRange(offset = 0, limit = LIBRARY_WINDOW_SIZE)

internal fun LibraryWindow<*>.visibleCountLabel(singular: String, plural: String): String {
    val noun = if (total == 1L) singular else plural
    return if (rows.size.toLong() < total) {
        "${rows.size} of $total $noun loaded"
    } else {
        "$total $noun"
    }
}

internal sealed interface LibraryScreenState {
    data class NoFolder(val message: String? = null) : LibraryScreenState

    data object TreeUnreadable : LibraryScreenState

    data class Scanning(
        val processed: ULong = 0u,
        val total: ULong? = null,
    ) : LibraryScreenState

    /**
     * [folderUri] is the tree the rows were read out of. It rides along because
     * the settings screen has to name the folder, and the session is the only
     * place that knows it — asking the port again from a composable would be a
     * second reader of a fact this state already carries.
     */
    data class Browse(
        val titles: LibraryWindow<LibraryTrack>,
        val albums: LibraryWindow<LibraryAlbum>,
        val artists: LibraryWindow<LibraryArtist>,
        val favourites: LibraryWindow<LibraryTrack> = LibraryWindow.empty(),
        val message: String? = null,
        val folderUri: String? = null,
        val loadedTabs: Set<BrowseTab> = BrowseTab.entries.toSet(),
    ) : LibraryScreenState
}

internal data class LibraryTrack(
    val id: Long,
    val uri: String,
    val title: String,
    val artist: String,
    val album: String,
    val durationMs: Long,
    val playCount: Long,
    val rating: Int,
)

internal data class LibraryAlbum(
    val title: String,
    val artist: String,
    val representativeUri: String,
    val trackCount: Long,
    val year: Int?,
    val totalDurationMs: Long,
)

internal data class LibraryArtist(
    val name: String,
    val trackCount: Long,
    val albumCount: Long,
    val representativeUri: String,
)

internal data class AlbumTrackList(
    val album: LibraryAlbum,
    val tracks: LibraryWindow<LibraryTrack>,
) {
    fun playbackSelection(startIndex: Int): PlaybackSelection =
        PlaybackSelection(tracks.rows, startIndex)
}

internal data class ArtistTrackList(
    val artist: LibraryArtist,
    val tracks: LibraryWindow<LibraryTrack>,
    val albums: LibraryWindow<LibraryAlbum> = LibraryWindow.empty(),
    val untaggedTracks: LibraryWindow<LibraryTrack> = LibraryWindow.empty(),
) {
    fun playbackSelection(startIndex: Int): PlaybackSelection =
        PlaybackSelection(tracks.rows, startIndex)
}

internal data class PlaybackSelection(
    val tracks: List<LibraryTrack>,
    val startIndex: Int,
)

internal sealed interface ScanProgressPresentation {
    data object Indeterminate : ScanProgressPresentation

    data class Determinate(val fraction: Float) : ScanProgressPresentation
}

internal fun LibraryScreenState.Scanning.progressPresentation(): ScanProgressPresentation {
    val knownTotal = total?.takeIf { it > 0u }
        ?: return ScanProgressPresentation.Indeterminate
    val fraction = (processed.toDouble() / knownTotal.toDouble()).coerceIn(0.0, 1.0)
    return ScanProgressPresentation.Determinate(fraction.toFloat())
}
