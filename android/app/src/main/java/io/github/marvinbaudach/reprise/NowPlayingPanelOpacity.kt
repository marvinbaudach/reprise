package io.github.marvinbaudach.reprise

import kotlin.math.pow

/**
 * How much of each layer a single now-playing panel shows.
 *
 * A panel stacks three things in the same square: the album [cover], the
 * spectrum [bars], and a blank [plate] that stands in for bars a panel cannot
 * draw. The three are decided together because they answer one question — what
 * fills this card right now — and deciding them apart is what let a cover show
 * through the visualizer during a swipe.
 */
internal data class NowPlayingPanelOpacity(
    val cover: Float,
    val bars: Float,
    val plate: Float,
)

/**
 * Splits a panel's square between cover, spectrum and plate.
 *
 * [visualizerOpacity] is the crossfade between "show the artwork" and "show the
 * visualizer"; it belongs to the whole sheet, not to one card. [near] is how
 * close this panel sits to the centre — 1 at rest in the middle, falling to 0
 * as it slides a full width away.
 *
 * The cover reads [visualizerOpacity] alone. It used to be damped by [near] as
 * well, which meant a card away from the centre kept its artwork: the incoming
 * neighbour arrived at full strength and the outgoing card faded its cover back
 * in as it left, so a swipe with the visualizer on showed two album covers the
 * visualizer was supposed to have replaced.
 *
 * The bars/plate split follows the geometric [near] falloff alone. A neighbour
 * panel's bars come from that track's precomputed offline spectrogram rather
 * than live PCM. Whatever the bars do not take, the plate does: `bars + plate`
 * is always [visualizerOpacity].
 */
internal fun nowPlayingPanelOpacity(
    visualizerOpacity: Float,
    near: Float,
): NowPlayingPanelOpacity {
    val visualizer = if (visualizerOpacity.isFinite()) visualizerOpacity.coerceIn(0f, 1f) else 0f
    val closeness = if (near.isFinite()) near.coerceIn(0f, 1f) else 0f
    val bars = visualizer * closeness.pow(BARS_FALLOFF)
    return NowPlayingPanelOpacity(
        cover = 1f - visualizer,
        bars = bars,
        plate = visualizer - bars,
    )
}

private const val BARS_FALLOFF = 1.4f
