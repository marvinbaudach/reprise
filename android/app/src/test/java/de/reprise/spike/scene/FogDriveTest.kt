package de.reprise.spike.scene

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.abs

class FogDriveTest {
    @Test
    fun the_cap_stays_an_order_of_magnitude_under_the_accessibility_limit() {
        // WCAG 2.3.1 permits three general flashes per second. This is not a
        // number to sit close to: it is a seizure threshold, and the fog is a
        // full-screen layer with nothing else competing for the eye.
        assertTrue(
            "a full-depth oscillation must stay far below three per second, got ${FogDrive.MAX_FLASH_HZ}",
            FogDrive.MAX_FLASH_HZ <= 0.3f,
        )
        assertEquals(
            1f / FogDrive.MAX_UNITS_PER_SECOND,
            FogDrive.FULL_RANGE_SECONDS,
            TOLERANCE,
        )
    }

    @Test
    fun one_step_never_moves_further_than_the_elapsed_time_allows() {
        listOf(1f / 120f, 1f / 60f, 1f / 20f, 0.5f).forEach { elapsed ->
            listOf(0f, 0.31f, 0.8f, 1f).forEach { start ->
                listOf(0f, 1f, 0.5f).forEach { target ->
                    val moved = abs(FogDrive.step(start, target, elapsed) - start)
                    assertTrue(
                        "moved $moved from $start towards $target in ${elapsed}s",
                        moved <= FogDrive.MAX_UNITS_PER_SECOND * elapsed + TOLERANCE,
                    )
                }
            }
        }
    }

    @Test
    fun the_cap_binds_in_both_directions() {
        val elapsed = 1f / 60f
        val room = FogDrive.MAX_UNITS_PER_SECOND * elapsed
        assertEquals(room, FogDrive.step(0f, 1f, elapsed), TOLERANCE)
        assertEquals(1f - room, FogDrive.step(1f, 0f, elapsed), TOLERANCE)
    }

    @Test
    fun a_target_within_reach_is_adopted_exactly_rather_than_approached() {
        // Otherwise the drive would creep towards a value it already has, and
        // every frame would report a change that nothing on screen can show.
        assertEquals(0.5f.toRawBits(), FogDrive.step(0.5f, 0.5f, 1f).toRawBits())
        assertEquals(0.52f.toRawBits(), FogDrive.step(0.5f, 0.52f, 1f).toRawBits())
    }

    @Test
    fun a_faster_alternation_moves_the_drive_less() {
        // The rate cap is what limits the depth, without anything here having
        // to recognise a beat: the quicker the signal changes its mind, the
        // less ground the drive covers before it is asked to turn around.
        fun swing(halfPeriodSeconds: Float): Float {
            var drive = 0.1f
            var lowest = Float.MAX_VALUE
            var highest = -Float.MAX_VALUE
            repeat(40) { half ->
                drive = FogDrive.step(drive, if (half % 2 == 0) 0.9f else 0.1f, halfPeriodSeconds)
                lowest = minOf(lowest, drive)
                highest = maxOf(highest, drive)
            }
            return highest - lowest
        }

        val fast = swing(1f / 8f)
        val slow = swing(1f)
        assertTrue("a 4 Hz alternation must barely move the drive, got $fast", fast < 0.16f)
        assertTrue("a 0.5 Hz alternation may still breathe, got $slow", slow > fast)
    }

    @Test
    fun a_stalled_clock_holds_the_drive_where_it_is() {
        listOf(0f, -1f, Float.NaN).forEach { elapsed ->
            assertEquals(0.42f.toRawBits(), FogDrive.step(0.42f, 1f, elapsed).toRawBits())
        }
    }

    @Test
    fun a_broken_reading_reads_as_silence_instead_of_poisoning_the_drive() {
        // A NaN would otherwise survive every later step and leave the fog
        // stuck for the rest of the track.
        listOf(Float.NaN, Float.POSITIVE_INFINITY).forEach { target ->
            val next = FogDrive.step(0.5f, target, 1f)
            assertTrue("got $next", next.isFinite())
        }
        assertEquals(0.5f.toRawBits(), FogDrive.step(Float.NaN, 0.5f, 1f).toRawBits())
    }

    @Test
    fun a_target_outside_the_range_is_clamped_rather_than_chased() {
        assertEquals(1f.toRawBits(), FogDrive.step(0.9f, 4f, 1f).toRawBits())
        assertEquals(0f.toRawBits(), FogDrive.step(0.1f, -4f, 1f).toRawBits())
    }

    private companion object {
        const val TOLERANCE = 0.000_01f
    }
}
