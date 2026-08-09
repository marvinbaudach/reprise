package de.reprise.spike

import de.reprise.spike.scene.CoreShape
import de.reprise.spike.scene.SceneState
import de.reprise.spike.scene.SpectrogramFrames
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

private fun driverFixture(sample: ScenePositionSample): DriverFixture {
    val frames = SpectrogramFrames(24, 20, ByteArray(24 * 80) { 180.toByte() })
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
