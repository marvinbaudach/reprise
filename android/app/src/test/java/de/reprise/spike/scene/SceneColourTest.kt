package de.reprise.spike.scene

import org.junit.Assert.assertEquals
import org.junit.Test

class SceneColourTest {
    @Test
    fun hue_is_fixed_to_clockwise_scene_angle() {
        assertEquals(250f, SceneColour.hue(0f), 0f)
        assertEquals(340f, SceneColour.hue(90f), 0f)
        assertEquals(70f, SceneColour.hue(180f), 0f)
        assertEquals(160f, SceneColour.hue(270f), 0f)
    }

    @Test
    fun hue_is_independent_of_every_band_value() {
        val silent = SceneState(SpectrogramFrames(24, 20, ByteArray(24)))
        val loud = SceneState(SpectrogramFrames(24, 20, ByteArray(24) { 255.toByte() }))
        silent.advanceTo(0)
        loud.advanceTo(0)

        val silentColour = SceneColour.hsl(90f, silent.motionLevel)
        val loudColour = SceneColour.hsl(90f, loud.motionLevel)
        assertEquals(silentColour.hue, loudColour.hue, 0f)
        assertEquals(0.95f, SceneColour.saturation, 0f)
        assertEquals(0.30f, silentColour.lightness, 0.0001f)
        assertEquals(0.56f, SceneColour.lightness(1f), 0.0001f)
    }
}
