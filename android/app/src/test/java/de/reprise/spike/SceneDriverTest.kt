package de.reprise.spike

import de.reprise.spike.scene.CoreShape
import de.reprise.spike.scene.SceneState
import de.reprise.spike.scene.SpectrogramFrames
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SceneDriverTest {
    @Test
    fun pause_produces_one_frame_then_no_more_revision_or_invalidation() {
        val fixture = driverFixture(
            sample = ScenePositionSample(positionMs = 500, observedAtNanos = 0, playing = false),
        )

        assertTrue(fixture.driver.tick())
        val revision = fixture.state.revision
        repeat(10) { assertFalse(fixture.driver.tick()) }

        assertEquals(revision, fixture.state.revision)
        assertEquals(10, fixture.driver.lastDrivenFrameIndex)
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
        val stepped = SceneState(frames, CoreShape("Track", "Artist"))
        (0..SceneState.CATCH_UP_FRAMES).forEach(stepped::advanceTo)
        assertEquals(SceneState.CATCH_UP_FRAMES, fixture.driver.lastDrivenFrameIndex)
        assertArrayEquals(stepped.fogBands, fixture.state.fogBands, 0f)
        assertArrayEquals(stepped.burstBands, fixture.state.burstBands, 0f)
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
        val stepped = SceneState(frames, CoreShape("Track", "Artist"))
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
    fun transition_is_the_only_non_revision_reason_to_invalidate() {
        val fixture = driverFixture(
            sample = ScenePositionSample(positionMs = 500, observedAtNanos = 0, playing = false),
        )
        fixture.driver.tick()

        assertFalse(fixture.driver.tick(transitionRunning = false))
        assertTrue(fixture.driver.tick(transitionRunning = true))
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

private fun driverFixture(
    sample: ScenePositionSample,
    frames: SpectrogramFrames = SpectrogramFrames(24, 20, ByteArray(24 * 80) { 180.toByte() }),
): DriverFixture {
    val state = SceneState(frames, CoreShape("Track", "Artist"))
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
