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
    fun liveAudioMovesAnUnanalysedSceneWithoutPositionOrSmoothedBands() {
        val frames = SpectrogramFrames(24, 20, ByteArray(0))
        val state = SceneState(frames)
        var nowNanos = 0L
        val received = mutableListOf<FloatArray?>()
        val sink = object : SceneFrameSink {
            override fun hasLiveAudio(): Boolean = true

            override fun bassPressure(): VisualBassPressure = VisualBassPressure.SILENT.copy(
                impact = 0.72f,
                aura = 0.44f,
                kick = 0.81f,
                pressure = 0.63f,
            )

            override fun onFrame(bands: FloatArray?) {
                received += bands?.copyOf()
            }
        }
        val driver = SceneDriver(
            frames = frames,
            state = state,
            clock = SceneClock { nowNanos },
            positionSource = ScenePositionSource {
                error("live audio must not estimate a spectrogram position")
            },
            frameSink = sink,
            framesAllowed = { true },
        )

        driver.tick()
        nowNanos = 50_000_000L
        driver.tick()

        assertEquals(listOf(null, null), received)
        assertEquals(0.63f, state.fogLevel, 0f)
        assertEquals(0.81f, state.bassPressure, 0f)
        assertEquals(0.81f, state.motionLevel, 0f)
        assertTrue(state.fogAngleA > 0f)
        assertNull(driver.lastDrivenFrameIndex)
    }

    @Test
    fun liveAudioWinsOverStoredSpectrogramUntilThePcmStreamDisappears() {
        val frames = SpectrogramFrames(24, 20, ByteArray(24) { 255.toByte() })
        val state = SceneState(frames)
        var live = true
        val received = mutableListOf<FloatArray?>()
        val sink = object : SceneFrameSink {
            override fun hasLiveAudio(): Boolean = live
            override fun bassPressure(): VisualBassPressure = VisualBassPressure.SILENT.copy(
                kick = 0.9f,
                pressure = 0.7f,
            )

            override fun onFrame(bands: FloatArray?) {
                received += bands?.copyOf()
            }
        }
        val driver = SceneDriver(
            frames = frames,
            state = state,
            clock = SceneClock { 0L },
            positionSource = ScenePositionSource { ScenePositionSample(0, 0, true) },
            frameSink = sink,
            framesAllowed = { true },
        )

        driver.tick()
        live = false
        driver.tick()

        assertNull(received.first())
        assertNotNull(received.last())
        assertEquals(24, received.last()?.size)
        assertEquals(0, driver.lastDrivenFrameIndex)
    }

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
    fun everyDisplayTickBetweenTwoFramesDrawsAValueInsideThatPairOfFrames() {
        val frames = risingFrames(frameCount = 20)
        var nowNanos = 0L
        val received = mutableListOf<FloatArray?>()
        val driver = SceneDriver(
            frames = frames,
            state = SceneState(frames),
            clock = SceneClock { nowNanos },
            positionSource = ScenePositionSource {
                ScenePositionSample(nowNanos / NANOS_PER_MILLISECOND, nowNanos, true)
            },
            frameSink = SceneFrameSink { bands -> received += bands?.copyOf() },
            framesAllowed = { true },
        )

        // Three display ticks inside one 20 Hz frame, then the frame after it.
        listOf(0L, 16_666_667L, 33_333_333L, 50_000_000L).forEach { nanos ->
            nowNanos = nanos
            driver.tick()
        }

        assertEquals(4, received.size)
        val drawn = received.filterNotNull()
        assertEquals("every tick draws bands", received.size, drawn.size)
        drawn.zipWithNext().forEach { (earlier, later) ->
            earlier.indices.forEach { band ->
                assertTrue(
                    "band $band stood still: ${earlier[band]} then ${later[band]}",
                    later[band] > earlier[band],
                )
            }
        }
        val thisFrame = drawn.first()
        val nextFrame = drawn.last()
        drawn.drop(1).dropLast(1).forEach { between ->
            between.indices.forEach { band ->
                assertTrue(
                    "band $band left its frame pair: ${between[band]}",
                    between[band] > thisFrame[band] && between[band] < nextFrame[band],
                )
            }
        }
    }

    @Test
    fun aPausedPlayheadFeedsTheVisualizerNothingBetweenFrames() {
        val frames = risingFrames(frameCount = 20)
        var nowNanos = 0L
        val received = mutableListOf<FloatArray?>()
        val driver = SceneDriver(
            frames = frames,
            state = SceneState(frames),
            clock = SceneClock { nowNanos },
            // Paused two thirds of the way into the first frame.
            positionSource = ScenePositionSource { ScenePositionSample(33, 0, false) },
            frameSink = SceneFrameSink { bands -> received += bands?.copyOf() },
            framesAllowed = { true },
        )

        driver.tick()
        repeat(5) {
            nowNanos += 16_666_667L
            driver.tick()
        }

        assertEquals(6, received.size)
        assertNotNull(received.first())
        assertTrue("a still playhead has nothing to feed", received.drop(1).all { it == null })
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

private const val NANOS_PER_MILLISECOND = 1_000_000L

/** A spectrogram whose every band climbs frame by frame, so any hold is visible. */
private fun risingFrames(frameCount: Int): SpectrogramFrames = SpectrogramFrames(
    bandCount = 24,
    frameRateHz = 20,
    cells = ByteArray(frameCount * 24) { index ->
        ((index / 24) * 12).coerceAtMost(255).toByte()
    },
)
