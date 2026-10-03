package io.github.marvinbaudach.reprise

import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.TextUnit

private const val TRACK_ROW_VERTICAL_PADDING_DP = 16f

internal data class LibraryFrameMetrics(
    val filterChipHeightDp: Int,
    val trackRowHeightDp: Int,
    val trackCoverSizeDp: Int,
    val miniPlayerHeightDp: Int,
    val navigationBarHeightDp: Int,
    val navigationRailWidthDp: Int = 80,
    val listColumns: Int = 1,
    val listColumnGapDp: Int = 0,
)

internal val libraryFrameMetrics = LibraryFrameMetrics(
    filterChipHeightDp = 32,
    trackRowHeightDp = 72,
    trackCoverSizeDp = 56,
    miniPlayerHeightDp = 72,
    navigationBarHeightDp = 80,
)

private val wideShortLibraryFrameMetrics = LibraryFrameMetrics(
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

internal fun effectiveTrackRowHeightDp(
    baseHeightDp: Int,
    density: Density,
    titleLineHeight: TextUnit,
    subtitleLineHeight: TextUnit,
): Float {
    if (density.fontScale <= 1f) return baseHeightDp.toFloat()
    val scaledTextBlockDp = with(density) {
        titleLineHeight.toDp().value + subtitleLineHeight.toDp().value
    }
    return maxOf(baseHeightDp.toFloat(), scaledTextBlockDp + TRACK_ROW_VERTICAL_PADDING_DP)
}

internal val libraryDestinations = BrowseTab.entries

internal data class TrackPlaybackPresentation(
    val isCurrent: Boolean,
    val animateBars: Boolean,
)

internal fun LibraryTrack.playbackPresentation(
    playback: LibraryPlayback,
): TrackPlaybackPresentation {
    val isCurrent = playback.currentTrackId == id
    return TrackPlaybackPresentation(
        isCurrent = isCurrent,
        animateBars = isCurrent && playback.isPlaying,
    )
}
