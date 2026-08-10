package de.reprise.spike.scene

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
        val raw = floor(positionMs.coerceAtLeast(0).toDouble() * frameRateHz / 1_000.0)
        return raw.coerceAtMost((frameCount - 1).toDouble()).toInt()
    }

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
}
