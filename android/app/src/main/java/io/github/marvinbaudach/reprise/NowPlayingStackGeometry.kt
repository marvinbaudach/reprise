package io.github.marvinbaudach.reprise

import kotlin.math.min

/** The Now Playing scene's own padding above and below its transport row. */
internal const val SCENE_TRANSPORT_PADDING_DP = 18

/** Where the seek bar sits when the screen is tall enough: a fraction of the height. */
internal const val SEEK_TOP_FRACTION = 0.69f

internal const val TITLE_TO_ARTIST_GAP_DP = 6

/** How far below the cover's centre the title sits when nothing presses on it. */
private const val TITLE_BELOW_COVER_CENTRE_DP = 156f
private const val SEEK_SLIDER_DP = 48f
private const val TRANSPORT_SIDE_BUTTON_DP = 48f
private const val BLOCK_GAP_DP = 8f
private const val COVER_TO_TITLE_GAP_DP = 16f

/** The header's icon row; the cover never rises into it. */
private const val COVER_TOP_MINIMUM_DP = 56f

/** Below this the cover gives up and the title drops to one line instead. */
private const val COVER_SCALE_FLOOR = 0.7f

/** A hard stop for a screen too short for any layout; the blocks then crowd regardless. */
private const val COVER_SCALE_MINIMUM = 0.5f
private const val TITLE_LINES_BUDGETED = 2

/**
 * What the stacked scene's vertical layout is made of, already resolved against
 * the font scale: text heights are in dp as they will be drawn, not in sp.
 */
internal data class NowPlayingStackInputs(
    val heightDp: Float,
    val navigationInsetDp: Float,
    val titleLineDp: Float,
    val artistLineDp: Float,
    val seekLabelDp: Float,
)

/**
 * Where the stacked scene puts its blocks. [coverScale] shrinks the cover about
 * [coverCentreYDp]; the other fields are top edges. [titleMaxLines] is what the
 * title may spend, so the budget the layout reserved is the budget it draws.
 */
internal data class NowPlayingStackGeometry(
    val coverCentreYDp: Float,
    val coverScale: Float,
    val titleTopDp: Float,
    val seekTopDp: Float,
    val titleMaxLines: Int,
)

/**
 * Lays the cover, title block, seek block and transport row out by their
 * heights instead of by fixed fractions of the screen.
 *
 * The transport row stays where the bottom edge and the navigation bar put it.
 * Everything above it keeps the position it has always had (the cover centred on
 * [PLAYED_CENTRE_FRACTION], the seek bar at [SEEK_TOP_FRACTION]) for as long as
 * that leaves room, and gives way only when it does not: the seek block rises to
 * clear the side buttons, the title block rises to clear the seek bar, and the
 * cover rises and then shrinks to clear the title. Text and touch targets never
 * shrink, since scaling them is the point of a large font.
 *
 * The room reserved for the title is [TITLE_LINES_BUDGETED] lines whatever the
 * track is called, so the cover does not change size between a short title and
 * a long one. When that budget would cost the cover more than [COVER_SCALE_FLOOR]
 * of its size, the title is held to one line and the budget shrinks with it.
 */
internal fun nowPlayingStackGeometry(inputs: NowPlayingStackInputs): NowPlayingStackGeometry {
    val full = stackGeometry(inputs, TITLE_LINES_BUDGETED)
    return if (full.coverScale >= COVER_SCALE_FLOOR) full else stackGeometry(inputs, 1)
}

private fun stackGeometry(inputs: NowPlayingStackInputs, titleLines: Int): NowPlayingStackGeometry {
    val height = inputs.heightDp
    val playButton = nowPlayingMetrics.playButtonSizeDp.toFloat()
    val transportTop = height - inputs.navigationInsetDp - SCENE_TRANSPORT_PADDING_DP - playButton
    // The side buttons are shorter than the play button and centred on it, so
    // the labels above them may reach down to the buttons' own top.
    val sideButtonsTop = transportTop + (playButton - TRANSPORT_SIDE_BUTTON_DP) / 2f
    val seekTop = min(
        height * SEEK_TOP_FRACTION,
        sideButtonsTop - BLOCK_GAP_DP - (SEEK_SLIDER_DP + inputs.seekLabelDp),
    )
    val titleBlock = titleLines * inputs.titleLineDp + TITLE_TO_ARTIST_GAP_DP + inputs.artistLineDp
    val titleTop = min(
        height * PLAYED_CENTRE_FRACTION + TITLE_BELOW_COVER_CENTRE_DP,
        seekTop - BLOCK_GAP_DP - titleBlock,
    )
    val coverBottom = titleTop - COVER_TO_TITLE_GAP_DP
    val restingTop = height * PLAYED_CENTRE_FRACTION - COVER_SIZE_DP / 2f
    val coverTop = (coverBottom - COVER_SIZE_DP).coerceIn(min(COVER_TOP_MINIMUM_DP, restingTop), restingTop)
    val coverSide = (coverBottom - coverTop).coerceIn(COVER_SIZE_DP * COVER_SCALE_MINIMUM, COVER_SIZE_DP.toFloat())
    return NowPlayingStackGeometry(
        coverCentreYDp = coverTop + coverSide / 2f,
        coverScale = coverSide / COVER_SIZE_DP,
        titleTopDp = titleTop,
        seekTopDp = seekTop,
        titleMaxLines = titleLines,
    )
}
