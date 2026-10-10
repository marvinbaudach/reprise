package io.github.marvinbaudach.reprise

import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/** The Now Playing scene's own padding above and below its transport row. */
private val SCENE_TRANSPORT_PADDING = SCENE_TRANSPORT_PADDING_DP.dp

/** The wide-short sheet's padding around its content column. */
private val WIDE_SHORT_SHEET_PADDING = 8.dp

/**
 * How far above the bottom edge a snackbar floats while the full-screen Now
 * Playing sheet is open: just over the transport row, whose top edge sits this
 * far from the bottom.
 *
 * Derived, not read: the row belongs to `NowPlayingScene` and the wide-short
 * sheet, which this surface does not own. It is the play button's height plus
 * the padding those layouts put under it, so a change there moves it; the layout
 * test `UndoSnackbarClearanceTest` compares it with the row's measured top.
 */
internal fun nowPlayingTransportTop(layout: SurfaceLayout): Dp = when (layout) {
    SurfaceLayout.STACKED ->
        nowPlayingMetrics(layout).playButtonSizeDp.dp + SCENE_TRANSPORT_PADDING
    SurfaceLayout.WIDE_SHORT ->
        nowPlayingMetrics(layout).playButtonSizeDp.dp + WIDE_SHORT_SHEET_PADDING
}

/**
 * Where the snackbar floats: above the transport row while the Now Playing
 * sheet is up, otherwise above whatever the library keeps along its bottom edge
 * ([libraryFrameInset]). The sheet lifts its transport clear of the navigation
 * bar, so the row's top is [sheetBottomInset] higher than its own layout puts it.
 */
internal fun undoSnackbarClearance(
    nowPlayingOpen: Boolean,
    layout: SurfaceLayout,
    libraryFrameInset: Dp,
    sheetBottomInset: Dp = 0.dp,
): Dp = if (nowPlayingOpen) {
    nowPlayingTransportTop(layout) + sheetBottomInset
} else {
    libraryFrameInset
}
