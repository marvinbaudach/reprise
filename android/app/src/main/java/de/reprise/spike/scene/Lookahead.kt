package de.reprise.spike.scene

import kotlin.math.max

object Lookahead {
    const val LOOKAHEAD_FRAMES = 8

    /** Future energy can lift a target; it can never pull the current value down. */
    fun target(frames: SpectrogramFrames, frameIndex: Int, band: Int): Float = max(
        frames.band(frameIndex, band),
        frames.band(saturatingAdd(frameIndex, LOOKAHEAD_FRAMES), band),
    ) / 255f

    private fun saturatingAdd(value: Int, amount: Int): Int =
        if (value > Int.MAX_VALUE - amount) Int.MAX_VALUE else value + amount
}
