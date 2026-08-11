package de.reprise.spike

import de.reprise.spike.scene.SceneState
import de.reprise.spike.scene.SpectrogramFrames
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
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

    @Test
    fun attachingAndDetachingTheVisualizerDoesNotResetFogMotion() {
        val frames = SpectrogramFrames(24, 20, ByteArray(48) { 96.toByte() })
        var nowNanos = 0L
        val state = SceneState(frames)
        val received = mutableListOf<FloatArray?>()
        val driver = SceneDriver(
            frames = frames,
            state = state,
            clock = SceneClock { nowNanos },
            positionSource = ScenePositionSource { ScenePositionSample(0, 0, true) },
            framesAllowed = { true },
        )

        driver.tick()
        nowNanos = 50_000_000L
        driver.tick()
        val angleBeforeSwitch = state.fogAngleA
        driver.setFrameSink(SceneFrameSink { bands -> received += bands?.copyOf() })
        driver.tick()
        assertNotNull(received.first())
        nowNanos = 100_000_000L
        driver.tick()
        driver.setFrameSink(null)
        nowNanos = 150_000_000L
        driver.tick()

        assertTrue(received.isNotEmpty())
        assertTrue(state.fogAngleA > angleBeforeSwitch)
        assertEquals(1, driver.lastDrivenFrameIndex)
    }
}
