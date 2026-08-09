package de.reprise.spike.scene

import kotlin.math.PI
import kotlin.math.sin

data class CoreShapeComponent(
    val harmonic: Int,
    val phaseRadians: Float,
    val amplitude: Float,
)

/** A deterministic irregular core edge derived once from track identity. */
class CoreShape(title: String, artist: String) {
    val coefficients: List<CoreShapeComponent> = components(stableHash("$title\u0000$artist"))

    fun radiusAt(thetaRadians: Float, baseRadius: Float, bass: Float): Float {
        val irregularity = coefficients.sumOf { component ->
            component.amplitude.toDouble() *
                sin(component.harmonic * thetaRadians.toDouble() + component.phaseRadians)
        }.toFloat()
        val breathing = bass.coerceIn(0f, 1f) * BASS_BREATH
        return baseRadius * (1f + irregularity + breathing)
    }

    private fun components(seed: Long): List<CoreShapeComponent> {
        var state = seed
        return AMPLITUDES.map { amplitude ->
            state = next(state)
            val harmonic = MIN_HARMONIC + (state % HARMONIC_COUNT).toInt()
            state = next(state)
            val phase = (state.toDouble() / UINT32_RANGE * 2.0 * PI).toFloat()
            CoreShapeComponent(harmonic, phase, amplitude)
        }
    }

    private fun stableHash(value: String): Long {
        var hash = FNV_OFFSET_BASIS
        value.toByteArray(Charsets.UTF_8).forEach { byte ->
            hash = (hash xor (byte.toInt() and 0xff).toLong()) * FNV_PRIME and UINT32_MASK
        }
        return hash
    }

    private fun next(value: Long): Long =
        (value * LCG_MULTIPLIER + LCG_INCREMENT) and UINT32_MASK

    private companion object {
        const val FNV_OFFSET_BASIS = 2_166_136_261L
        const val FNV_PRIME = 16_777_619L
        const val UINT32_MASK = 0xffff_ffffL
        const val UINT32_RANGE = 4_294_967_296.0
        const val LCG_MULTIPLIER = 1_664_525L
        const val LCG_INCREMENT = 1_013_904_223L
        const val MIN_HARMONIC = 3
        const val HARMONIC_COUNT = 7L
        const val BASS_BREATH = 0.12f
        val AMPLITUDES = listOf(0.06f, 0.04f, 0.03f)
    }
}
