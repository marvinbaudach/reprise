package de.reprise.spike

import de.reprise.spike.scene.SceneState
import de.reprise.spike.scene.SpectrogramFrames
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SceneDriverTest {
    @Test
    fun shimmer_clock_turns_without_analysis_and_keeps_only_the_current_minute() {
        val fixture = driverFixture(
            sample = ScenePositionSample(positionMs = 0, observedAtNanos = 0, playing = true),
            frames = SpectrogramFrames(24, 20, ByteArray(0)),
        )
        fixture.driver.tick()

        fixture.clock.now = 30_000_000_000
        fixture.driver.tick()
        assertEquals(30.0, fixture.state.shimmerElapsedSeconds, 0.000_001)

        fixture.clock.now = 60_000_000_000
        fixture.driver.tick()
        assertEquals(0.0, fixture.state.shimmerElapsedSeconds, 0.000_001)
    }

    @Test
    fun driver_reads_fast_bass_pressure_at_the_display_fraction_and_pause_holds_it() {
        val cells = ByteArray(10 * 24) { index ->
            val frame = index / 24
            val band = index % 24
            if (frame == 9 && band < 7) 255.toByte() else 0
        }
        val fixture = driverFixture(
            sample = ScenePositionSample(positionMs = 25, observedAtNanos = 0, playing = false),
            frames = SpectrogramFrames(bandCount = 24, frameRateHz = 20, cells = cells),
        )

        fixture.driver.tick()
        val halfway = fixture.state.bassPressure
        val revision = fixture.state.revision
        fixture.clock.now = 50_000_000
        fixture.driver.tick()

        assertTrue(halfway > 0f)
        assertEquals(halfway.toRawBits(), fixture.state.bassPressure.toRawBits())
        assertTrue(fixture.state.revision > revision)
    }

    @Test
    fun paused_non_empty_fog_keeps_turning_at_the_base_drift_rate() {
        val fixture = driverFixture(
            sample = ScenePositionSample(positionMs = 500, observedAtNanos = 0, playing = false),
        )

        assertTrue(fixture.driver.tick())
        val startAngle = fixture.state.fogAngleA
        repeat(20) {
            fixture.clock.now += SPECTROGRAM_FRAME_MS * NANOS_PER_MILLISECOND
            assertTrue(fixture.driver.tick())
        }

        assertTrue(fixture.state.fogAngleA > startAngle)
        assertEquals(
            SceneState.FOG_BASE_DEGREES_PER_SECOND,
            fixture.state.fogAngleA - startAngle,
            0.001f,
        )
        assertEquals(10, fixture.driver.lastDrivenFrameIndex)
    }

    @Test
    fun strong_music_turns_the_fog_at_least_five_times_faster_than_base_drift() {
        val paused = driverFixture(
            sample = ScenePositionSample(positionMs = 0, observedAtNanos = 0, playing = false),
            frames = constantFrames(cell = 255, frameCount = 24),
        )
        val strong = driverFixture(
            sample = ScenePositionSample(positionMs = 0, observedAtNanos = 0, playing = true),
            frames = constantFrames(cell = 255, frameCount = 24),
        )
        paused.driver.tick()
        strong.driver.tick()
        val pausedStart = paused.state.fogAngleA
        val strongStart = strong.state.fogAngleA

        repeat(20) { step ->
            val elapsedMs = (step + 1) * SPECTROGRAM_FRAME_MS
            paused.clock.now = elapsedMs * NANOS_PER_MILLISECOND
            strong.clock.now = elapsedMs * NANOS_PER_MILLISECOND
            strong.source.sample = strong.source.sample.copy(
                positionMs = elapsedMs,
                observedAtNanos = strong.clock.now,
            )
            paused.driver.tick()
            strong.driver.tick()
        }

        val pausedChange = paused.state.fogAngleA - pausedStart
        val strongChange = strong.state.fogAngleA - strongStart
        assertTrue(
            "strong change $strongChange must be at least five times base $pausedChange",
            strongChange >= pausedChange * 5f,
        )
        assertTrue(strong.state.fogAngleB < paused.state.fogAngleB)
    }

    @Test
    fun paused_frame_loop_requests_no_more_than_twenty_callbacks_per_second() {
        assertTrue(1_000L / PAUSED_SCENE_FRAME_INTERVAL_MS <= 20L)
    }

    @Test
    fun screen_off_stops_stepping_even_while_position_and_clock_advance() {
        val fixture = driverFixture(
            sample = ScenePositionSample(positionMs = 0, observedAtNanos = 0, playing = true),
        )
        fixture.driver.tick()
        val revision = fixture.state.revision
        val frame = fixture.driver.lastDrivenFrameIndex

        fixture.power.framesAllowed = false
        fixture.clock.now = 900_000_000
        fixture.source.sample = fixture.source.sample.copy(positionMs = 500)

        repeat(4) { assertFalse(fixture.driver.tick()) }
        assertEquals(revision, fixture.state.revision)
        assertEquals(frame, fixture.driver.lastDrivenFrameIndex)
    }

    @Test
    fun a_screen_off_gap_is_replayed_on_resume_instead_of_being_read_as_a_seek() {
        val frames = patternedFrames(frameCount = SceneState.CATCH_UP_FRAMES + 2)
        val fixture = driverFixture(
            sample = ScenePositionSample(positionMs = 0, observedAtNanos = 0, playing = true),
            frames = frames,
        )
        fixture.driver.tick()

        fixture.power.framesAllowed = false
        repeat(3) { assertFalse(fixture.driver.tick()) }
        fixture.power.framesAllowed = true
        fixture.source.sample = fixture.source.sample.copy(
            positionMs = SceneState.CATCH_UP_FRAMES * SPECTROGRAM_FRAME_MS,
        )

        assertTrue(fixture.driver.tick())
        val stepped = SceneState(frames)
        (0..SceneState.CATCH_UP_FRAMES).forEach(stepped::advanceTo)
        assertEquals(SceneState.CATCH_UP_FRAMES, fixture.driver.lastDrivenFrameIndex)
        assertArrayEquals(stepped.fogBands, fixture.state.fogBands, 0f)
        assertArrayEquals(stepped.motionBands, fixture.state.motionBands, 0f)
        assertEquals(stepped.fogAngleA.toRawBits(), fixture.state.fogAngleA.toRawBits())
        assertEquals(stepped.fogAngleB.toRawBits(), fixture.state.fogAngleB.toRawBits())
    }

    @Test
    fun a_frame_loop_torn_down_by_the_gate_reports_its_gap_without_ticking() {
        val frames = patternedFrames(frameCount = GAP_FRAMES + 2)
        val fixture = driverFixture(
            sample = ScenePositionSample(positionMs = 0, observedAtNanos = 0, playing = true),
            frames = frames,
        )
        fixture.driver.tick()

        fixture.driver.noteFramesWithheld()
        fixture.source.sample = fixture.source.sample.copy(
            positionMs = GAP_FRAMES * SPECTROGRAM_FRAME_MS,
        )

        assertTrue(fixture.driver.tick())
        val stepped = SceneState(frames)
        (0..GAP_FRAMES).forEach(stepped::advanceTo)
        assertArrayEquals(stepped.fogBands, fixture.state.fogBands, 0f)
        assertEquals(stepped.fogAngleA.toRawBits(), fixture.state.fogAngleA.toRawBits())
    }

    @Test
    fun a_gap_wider_than_the_cap_snaps_rather_than_spending_the_resume_on_it() {
        val frames = patternedFrames(frameCount = SceneState.CATCH_UP_FRAMES + 2)
        val beyondCap = SceneState.CATCH_UP_FRAMES + 1
        val fixture = driverFixture(
            sample = ScenePositionSample(positionMs = 0, observedAtNanos = 0, playing = true),
            frames = frames,
        )
        fixture.driver.tick()
        val angleAfterFirstFrame = fixture.state.fogAngleA.toRawBits()

        fixture.power.framesAllowed = false
        fixture.driver.tick()
        fixture.power.framesAllowed = true
        fixture.source.sample = fixture.source.sample.copy(
            positionMs = beyondCap * SPECTROGRAM_FRAME_MS,
        )

        assertTrue(fixture.driver.tick())
        assertEquals(beyondCap, fixture.driver.lastDrivenFrameIndex)
        assertEquals(angleAfterFirstFrame, fixture.state.fogAngleA.toRawBits())
    }

    @Test
    fun interpolation_is_capped_to_the_measured_500_ms_media3_cadence() {
        val fixture = driverFixture(
            sample = ScenePositionSample(positionMs = 0, observedAtNanos = 0, playing = true),
        )

        fixture.clock.now = 250_000_000
        fixture.driver.tick()
        assertEquals(5, fixture.driver.lastDrivenFrameIndex)

        fixture.clock.now = 900_000_000
        fixture.driver.tick()
        assertEquals(10, fixture.driver.lastDrivenFrameIndex)
        assertEquals(500L, SceneDriver.measuredPositionIntervalMs)
    }

    @Test
    fun measured_driver_cadence_matches_the_media3_position_publisher() {
        val media3Source = java.io.File(
            "src/main/java/de/reprise/spike/Media3PlaybackPort.kt",
        ).readText()

        assertTrue("POSITION_INTERVAL_MS = 500L" in media3Source)
        assertEquals(500L, SceneDriver.measuredPositionIntervalMs)
    }

    @Test
    fun a_live_sink_reports_only_pressure_that_changed_the_scene() {
        val frames = SpectrogramFrames(24, 20, ByteArray(0))
        val state = SceneState(frames)
        var pressure = VisualBassPressure.SILENT
        val sink = object : SceneFrameSink {
            override fun hasLiveAudio(): Boolean = true

            override fun bassPressure(): VisualBassPressure = pressure

            override fun onFrame(bands: FloatArray?) = Unit
        }
        val driver = SceneDriver(
            frames = frames,
            state = state,
            clock = SceneClock { 0L },
            positionSource = ScenePositionSource {
                error("live audio must not read the stored playhead")
            },
            frameSink = sink,
            framesAllowed = { true },
        )

        assertFalse(driver.tick())
        pressure = pressure.copy(kick = 0.8f, pressure = 0.6f)
        assertTrue(driver.tick())
    }

}

private data class DriverFixture(
    val state: SceneState,
    val clock: FakeSceneClock,
    val source: FakeScenePositionSource,
    val power: FakeScenePower,
    val driver: SceneDriver,
)

/** One spectrogram frame at the 20 Hz frame rate the fixtures below use. */
private const val SPECTROGRAM_FRAME_MS = 50L
private const val NANOS_PER_MILLISECOND = 1_000_000L

/** A gap the phone spends in a pocket: ten seconds, comfortably inside the cap. */
private const val GAP_FRAMES = 200

private fun patternedFrames(frameCount: Int): SpectrogramFrames = SpectrogramFrames(
    bandCount = 24,
    frameRateHz = 20,
    cells = ByteArray(frameCount * 24) { index ->
        val frame = index / 24
        val band = index % 24
        ((frame * 17 + band * 7) % 256).toByte()
    },
)

private fun constantFrames(cell: Int, frameCount: Int): SpectrogramFrames = SpectrogramFrames(
    bandCount = 24,
    frameRateHz = 20,
    cells = ByteArray(frameCount * 24) { cell.toByte() },
)

private fun driverFixture(
    sample: ScenePositionSample,
    frames: SpectrogramFrames = SpectrogramFrames(24, 20, ByteArray(24 * 80) { 180.toByte() }),
): DriverFixture {
    val state = SceneState(frames)
    val clock = FakeSceneClock()
    val source = FakeScenePositionSource(sample)
    val power = FakeScenePower()
    return DriverFixture(
        state = state,
        clock = clock,
        source = source,
        power = power,
        driver = SceneDriver(frames, state, clock, source) { power.framesAllowed },
    )
}

private class FakeSceneClock(var now: Long = 0) : SceneClock {
    override fun nowNanos(): Long = now
}

private class FakeScenePositionSource(var sample: ScenePositionSample) : ScenePositionSource {
    override fun current(): ScenePositionSample = sample
}

private class FakeScenePower(var framesAllowed: Boolean = true)
