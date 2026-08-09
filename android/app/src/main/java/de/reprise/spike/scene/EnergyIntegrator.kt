package de.reprise.spike.scene

object EnergyIntegrator {
    fun advance(angle: Float, energy: Float, factor: Float): Float =
        wrap360(angle + energy.coerceIn(0f, 1f) * factor)

    fun wrap360(angle: Float): Float {
        val wrapped = angle % 360f
        return if (wrapped < 0f) wrapped + 360f else wrapped
    }
}
