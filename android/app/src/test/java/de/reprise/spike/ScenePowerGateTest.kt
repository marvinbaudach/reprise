package de.reprise.spike

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ScenePowerGateTest {
    @Test
    fun animations_off_suppresses_exactly_fog_rotation_bloom_and_hot_ray() {
        val controller = AmbientMotionController()
        controller.attach()
        controller.runtimeChanged(
            resumed = true,
            screenInteractive = true,
            animationsEnabled = false,
        )

        val power = controller.sceneRenderPower()

        assertFalse(power.fogRotates)
        assertFalse(power.burstEffects.bloom)
        assertFalse(power.burstEffects.hotRay)
        assertTrue(power.coronaKeepsCurrentSignal)
    }

    @Test
    fun resumed_interactive_animations_enable_all_three_effects() {
        val controller = AmbientMotionController()
        controller.attach()
        controller.runtimeChanged(
            resumed = true,
            screenInteractive = true,
            animationsEnabled = true,
        )

        val power = controller.sceneRenderPower()

        assertTrue(power.fogRotates)
        assertTrue(power.burstEffects.bloom)
        assertTrue(power.burstEffects.hotRay)
        assertTrue(power.coronaKeepsCurrentSignal)
    }
}
