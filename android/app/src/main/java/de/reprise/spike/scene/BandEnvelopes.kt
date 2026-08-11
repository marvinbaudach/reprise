package de.reprise.spike.scene

import kotlin.math.exp

/** One one-pole attack/decay follower per spectrogram band. */
class BandEnvelopes(
    bandCount: Int,
    frameMs: Float,
    attackMs: Float,
    decayMs: Float,
) {
    val values = FloatArray(bandCount)
    private val attackCoefficient = coefficient(frameMs, attackMs)
    private val decayCoefficient = coefficient(frameMs, decayMs)

    init {
        require(bandCount > 0) { "bandCount must be positive" }
        require(frameMs > 0f) { "frameMs must be positive" }
        require(attackMs > 0f) { "attackMs must be positive" }
        require(decayMs > 0f) { "decayMs must be positive" }
    }

    fun step(targets: FloatArray): Boolean {
        var changed = false
        values.indices.forEach { band ->
            val current = values[band]
            val target = targets.getOrElse(band) { 0f }.coerceIn(0f, 1f)
            val coefficient = if (target > current) attackCoefficient else decayCoefficient
            val next = current + (target - current) * coefficient
            if (next.toRawBits() != current.toRawBits()) {
                values[band] = next
                changed = true
            }
        }
        return changed
    }

    fun adopt(raw: FloatArray): Boolean {
        var changed = false
        values.indices.forEach { band ->
            val next = raw.getOrElse(band) { 0f }.coerceIn(0f, 1f)
            if (next.toRawBits() != values[band].toRawBits()) {
                values[band] = next
                changed = true
            }
        }
        return changed
    }

    companion object {
        private const val FOG_ATTACK_MS = 200f
        private const val FOG_DECAY_MS = 1_200f
        private const val MOTION_ATTACK_MS = 40f
        private const val MOTION_DECAY_MS = 220f

        fun fog(bandCount: Int, frameRateHz: Int): BandEnvelopes = BandEnvelopes(
            bandCount = bandCount,
            frameMs = frameMs(frameRateHz),
            attackMs = FOG_ATTACK_MS,
            decayMs = FOG_DECAY_MS,
        )

        fun motion(bandCount: Int, frameRateHz: Int): BandEnvelopes = BandEnvelopes(
            bandCount = bandCount,
            frameMs = frameMs(frameRateHz),
            attackMs = MOTION_ATTACK_MS,
            decayMs = MOTION_DECAY_MS,
        )

        private fun frameMs(frameRateHz: Int): Float {
            require(frameRateHz > 0) { "frameRateHz must be positive" }
            return 1_000f / frameRateHz.toFloat()
        }

        private fun coefficient(frameMs: Float, timeMs: Float): Float =
            (1.0 - exp(-frameMs.toDouble() / timeMs.toDouble())).toFloat()
    }
}
