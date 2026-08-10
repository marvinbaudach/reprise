package de.reprise.spike.scene

data class SceneHsl(
    val hue: Float,
    val saturation: Float,
    val lightness: Float,
)

object SceneColour {
    const val saturation = 0.95f

    fun hue(angleDegClockwiseFromTop: Float): Float =
        EnergyIntegrator.wrap360(250f + angleDegClockwiseFromTop)

    fun lightness(energy: Float): Float = 0.30f + energy.coerceIn(0f, 1f) * 0.26f

    fun hsl(angleDegClockwiseFromTop: Float, energy: Float): SceneHsl = SceneHsl(
        hue = hue(angleDegClockwiseFromTop),
        saturation = saturation,
        lightness = lightness(energy),
    )
}
