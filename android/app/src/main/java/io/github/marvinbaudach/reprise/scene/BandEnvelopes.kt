package io.github.marvinbaudach.reprise.scene

import kotlin.math.exp

/** One one-pole attack/decay follower per spectrogram band. */
class BandEnvelopes(
    bandCount: Int,
    private val frameMs: Float,
    private val attackMs: Float,
    private val decayMs: Float,
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

    /**
     * Fills [into] with the values this follower holds once [fraction] of its
     * step towards [targets] has passed, without moving the follower itself.
     *
     * A one-pole's decay composes: `fraction = 1f` lands on exactly what [step]
     * writes, and every smaller fraction sits between the current value and
     * that one. So this reads out the curve the follower already travels
     * between two measured frames — it invents no value beyond either end.
     */
    fun projectInto(into: FloatArray, targets: FloatArray, fraction: Float) {
        val part = fraction.coerceIn(0f, 1f)
        val attack = coefficient(frameMs * part, attackMs)
        val decay = coefficient(frameMs * part, decayMs)
        into.indices.forEach { band ->
            val current = values.getOrElse(band) { 0f }
            val target = targets.getOrElse(band) { 0f }.coerceIn(0f, 1f)
            into[band] = current + (target - current) * if (target > current) attack else decay
        }
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
