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
 * Only the playing track has PCM to draw, so [hasVisualizer] decides whether
 * this panel can show bars at all, and the bars that do show still fall off
 * with [near] so a card leaving the centre quietens rather than cuts. Whatever
 * the bars do not take, the plate does: `bars + plate` is always
 * [visualizerOpacity], so a panel with no spectrum of its own shows a blank
 * card instead of a hole where the cover used to be.
 */
internal fun nowPlayingPanelOpacity(
    visualizerOpacity: Float,
    near: Float,
    hasVisualizer: Boolean,
): NowPlayingPanelOpacity {
    val visualizer = visualizerOpacity.coerceIn(0f, 1f)
    val closeness = near.coerceIn(0f, 1f)
    val bars = if (hasVisualizer) visualizer * closeness.pow(BARS_FALLOFF) else 0f
    return NowPlayingPanelOpacity(
        cover = 1f - visualizer,
        bars = bars,
        plate = visualizer - bars,
    )
}

private const val BARS_FALLOFF = 1.4f
