package de.reprise.spike

internal data class LibraryFrameMetrics(
    val topAppBarHeightDp: Int,
    val filterChipHeightDp: Int,
    val trackRowHeightDp: Int,
    val trackCoverSizeDp: Int,
    val miniPlayerHeightDp: Int,
    val navigationBarHeightDp: Int,
)

internal val libraryFrameMetrics = LibraryFrameMetrics(
    topAppBarHeightDp = 64,
    filterChipHeightDp = 32,
    trackRowHeightDp = 72,
    trackCoverSizeDp = 56,
    miniPlayerHeightDp = 72,
    navigationBarHeightDp = 80,
)

internal enum class LibraryDestination(
    val label: String,
    val symbol: String,
) {
    LIBRARY("Library", "library_music"),
}

internal val libraryDestinations = listOf(LibraryDestination.LIBRARY)

internal data class TrackPlaybackPresentation(
    val isCurrent: Boolean,
    val animateBars: Boolean,
)

internal fun LibraryTrack.playbackPresentation(
    playback: PlaybackUiState,
): TrackPlaybackPresentation {
    val isCurrent = playback.currentTrackId == id
    return TrackPlaybackPresentation(
        isCurrent = isCurrent,
        animateBars = isCurrent && playback.isPlaying,
    )
}
