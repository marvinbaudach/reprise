package de.reprise.spike

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ScenePowerGateTest {
    @Test
    fun animations_off_suppresses_fog_rotation_without_stopping_scene_frames() {
        val controller = AmbientMotionController()
        controller.attach()
        controller.runtimeChanged(
            resumed = true,
            screenInteractive = true,
            animationsEnabled = false,
        )

        val power = controller.sceneRenderPower()

        assertFalse(power.fogRotates)
        assertTrue(controller.sceneFramesAllowed)
    }

    @Test
    fun resumed_interactive_animations_enable_fog_rotation() {
        val controller = AmbientMotionController()
        controller.attach()
        controller.runtimeChanged(
            resumed = true,
            screenInteractive = true,
            animationsEnabled = true,
        )

        val power = controller.sceneRenderPower()

        assertTrue(power.fogRotates)
    }

    /**
     * The other gate, and the harder one: animations-off only takes the three
     * effects away, while a backgrounded activity or a dark screen must stop
     * the scene from being drawn at all. That is [SceneDriver]'s first line, so
     * what these two assert is the answer it asks for.
     */
    @Test
    fun a_backgrounded_activity_stops_scene_frames_altogether() {
        val controller = AmbientMotionController()
        controller.attach()
        controller.runtimeChanged(
            resumed = false,
            screenInteractive = true,
            animationsEnabled = true,
        )

        assertFalse(controller.sceneFramesAllowed)
        assertFalse(controller.sceneAnimationsEnabled)
    }

    @Test
    fun a_dark_screen_stops_scene_frames_altogether() {
        val controller = AmbientMotionController()
        controller.attach()
        controller.runtimeChanged(
            resumed = true,
            screenInteractive = false,
            animationsEnabled = true,
        )

        assertFalse(controller.sceneFramesAllowed)
        assertFalse(controller.sceneAnimationsEnabled)
    }
}
