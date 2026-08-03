package de.reprise.spike

internal sealed interface LibraryScreenState {
    data class NoFolder(val message: String? = null) : LibraryScreenState

    data object TreeUnreadable : LibraryScreenState

    data class Scanning(
        val processed: ULong = 0u,
        val total: ULong? = null,
    ) : LibraryScreenState

    data class Browse(
        val titles: List<LibraryTrack>,
        val albums: List<LibraryAlbum>,
        val artists: List<LibraryArtist>,
        val message: String? = null,
    ) : LibraryScreenState
}

internal data class LibraryTrack(
    val uri: String,
    val title: String,
    val artist: String,
    val album: String,
    val durationMs: Long,
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
    val tracks: List<LibraryTrack>,
) {
    fun playbackSelection(startIndex: Int): PlaybackSelection =
        PlaybackSelection(tracks, startIndex)
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
