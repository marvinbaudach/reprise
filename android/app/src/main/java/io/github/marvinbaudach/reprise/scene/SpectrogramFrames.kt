package io.github.marvinbaudach.reprise.scene

import kotlin.math.floor

/** Immutable frame-major spectrogram data supplied by Rust. */
class SpectrogramFrames(
    val bandCount: Int,
    val frameRateHz: Int,
    cells: ByteArray,
) {
    private val cells = cells.copyOf()

    init {
        require(bandCount > 0) { "bandCount must be positive" }
        require(frameRateHz > 0) { "frameRateHz must be positive" }
        require(cells.size % bandCount == 0) {
            "cells must be whole frames: ${cells.size} cells over $bandCount bands"
        }
    }

    val frameCount: Int = cells.size / bandCount

    fun frameIndexFor(positionMs: Long): Int {
        if (frameCount == 0) return 0
        val raw = floor(framePosition(positionMs))
        return raw.coerceAtMost((frameCount - 1).toDouble()).toInt()
    }

    /**
     * How far the playhead stands inside the frame [frameIndexFor] reports, as
     * a 0..1 fraction of one frame.
     *
     * Zero on the last frame and past the end of the analysis: there is no
     * measured frame left to travel towards, so nothing may move.
     */
    fun frameFractionFor(positionMs: Long): Float {
        if (frameCount == 0) return 0f
        val raw = framePosition(positionMs)
        if (raw >= (frameCount - 1).toDouble()) return 0f
        return (raw - floor(raw)).toFloat()
    }

    private fun framePosition(positionMs: Long): Double =
        positionMs.coerceAtLeast(0).toDouble() * frameRateHz / 1_000.0

    fun band(frameIndex: Int, band: Int): Int {
        if (frameCount == 0) return 0
        val clampedFrame = clampFrameIndex(frameIndex)
        val clampedBand = band.coerceIn(0, bandCount - 1)
        return cells[clampedFrame * bandCount + clampedBand].toInt() and 0xff
    }

    internal fun clampFrameIndex(frameIndex: Int): Int = when {
        frameCount == 0 -> 0
        else -> frameIndex.coerceIn(0, frameCount - 1)
    }

    companion object {
        /**
         * The shape every analysis arrives in, Rust's `SPECTROGRAM_BAND_COUNT` and
         * `SPECTROGRAM_FRAME_RATE_HZ`. Kept here once so the empty stand-in cannot drift
         * from the real frames: a scene keyed on a different shape would restart. The
         * Rust test `nav_15d_the_scene_stand_in_has_the_spectrogram_shape` reads these
         * two lines and fails when they no longer match.
         */
        const val BAND_COUNT = 24
        const val FRAME_RATE_HZ = 20

        /** No analysis yet, in the shape the real one will have. */
        fun empty() = SpectrogramFrames(BAND_COUNT, FRAME_RATE_HZ, ByteArray(0))
    }
}
