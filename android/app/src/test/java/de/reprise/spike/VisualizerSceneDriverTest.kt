package de.reprise.spike

import de.reprise.spike.scene.SceneState
import de.reprise.spike.scene.SpectrogramFrames
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class VisualizerSceneDriverTest {
    @Test
    fun theExistingDriverFeedsOneSmoothedFrameThenTicksWithoutRepeatingIt() {
        val frames = SpectrogramFrames(24, 20, ByteArray(24) { (it * 9).toByte() })
        val received = mutableListOf<FloatArray?>()
        val driver = SceneDriver(
            frames = frames,
            state = SceneState(frames),
            clock = SceneClock { 0L },
            positionSource = ScenePositionSource { ScenePositionSample(0, 0, true) },
            frameSink = SceneFrameSink { bands -> received += bands?.copyOf() },
            framesAllowed = { true },
        )

        driver.tick()
        driver.tick()

        assertEquals(2, received.size)
        assertEquals(24, received.first()?.size)
        assertTrue(received.first()!!.any { it > 0f })
        assertNull(received.last())
    }

    @Test
    fun noAnalysisIsDeclaredOnceAndThePowerGateWithholdsEveryVisualizerFrame() {
        val frames = SpectrogramFrames(24, 20, ByteArray(0))
        val received = mutableListOf<FloatArray?>()
        var allowed = true
        val driver = SceneDriver(
            frames = frames,
            state = SceneState(frames),
            clock = SceneClock { 0L },
            positionSource = ScenePositionSource { ScenePositionSample(0, 0, true) },
            frameSink = SceneFrameSink { bands -> received += bands?.copyOf() },
            framesAllowed = { allowed },
        )

        driver.tick()
        driver.tick()
        allowed = false
        driver.tick()

        assertEquals(2, received.size)
        assertTrue(received.first()!!.isEmpty())
        assertNull(received.last())
    }
}
