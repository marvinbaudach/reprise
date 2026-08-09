package de.reprise.spike.scene

import kotlin.math.sin
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertSame
import org.junit.Test

class CoreShapeTest {
    @Test
    fun core_shape_is_stable_per_track_and_never_changes_while_advancing() {
        val first = CoreShape("Glass City", "Reprise")
        val replay = CoreShape("Glass City", "Reprise")
        val anotherTrack = CoreShape("Different City", "Reprise")

        assertEquals(first.coefficients, replay.coefficients)
        assertEquals(first.radiusAt(1.25f, 78f, 0.4f), replay.radiusAt(1.25f, 78f, 0.4f), 0f)
        assertNotEquals(first.coefficients, anotherTrack.coefficients)

        val state = SceneState(
            SpectrogramFrames(24, 20, ByteArray(24 * 16) { 128.toByte() }),
            first,
        )
        state.advanceTo(12)
        assertSame(first, state.coreShape)
        assertEquals(first.coefficients, state.coreShape.coefficients)
    }

    @Test
    fun the_radius_is_the_sum_over_exactly_the_published_coefficients() {
        val shape = CoreShape("Glass City", "Reprise")

        listOf(-2.5f, 0f, 0.75f, 1.25f, 4f).forEach { theta ->
            val irregularity = shape.coefficients.sumOf { component ->
                component.amplitude.toDouble() *
                    sin(component.harmonic * theta.toDouble() + component.phaseRadians)
            }.toFloat()

            assertEquals(78f * (1f + irregularity), shape.radiusAt(theta, 78f, 0f), 0f)
        }
    }
}
