package de.reprise.spike

internal data class LibraryFrameMetrics(
    val topAppBarHeightDp: Int,
    val filterChipHeightDp: Int,
    val trackRowHeightDp: Int,
    val trackCoverSizeDp: Int,
    val miniPlayerHeightDp: Int,
    val navigationBarHeightDp: Int,
    val navigationRailWidthDp: Int = 80,
    val navigationRailIndicatorWidthDp: Int = 56,
    val navigationRailIndicatorHeightDp: Int = 32,
    val listColumns: Int = 1,
    val listColumnGapDp: Int = 0,
)

internal val libraryFrameMetrics = LibraryFrameMetrics(
    topAppBarHeightDp = 64,
    filterChipHeightDp = 32,
    trackRowHeightDp = 72,
    trackCoverSizeDp = 56,
    miniPlayerHeightDp = 72,
    navigationBarHeightDp = 80,
)

private val wideShortLibraryFrameMetrics = LibraryFrameMetrics(
    topAppBarHeightDp = 52,
    filterChipHeightDp = 32,
    trackRowHeightDp = 64,
    trackCoverSizeDp = 48,
    miniPlayerHeightDp = 72,
    navigationBarHeightDp = 80,
    listColumns = 2,
    listColumnGapDp = 16,
)

internal fun libraryFrameMetrics(layout: SurfaceLayout): LibraryFrameMetrics = when (layout) {
    SurfaceLayout.STACKED -> libraryFrameMetrics
    SurfaceLayout.WIDE_SHORT -> wideShortLibraryFrameMetrics
}

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
