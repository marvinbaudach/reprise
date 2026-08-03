package de.reprise.spike

internal sealed interface LibraryScreenState {
    data class NoFolder(val message: String? = null) : LibraryScreenState

    data class Scanning(
        val processed: ULong = 0u,
        val total: ULong? = null,
    ) : LibraryScreenState

    data class TrackList(
        val tracks: List<LibraryTrack>,
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
