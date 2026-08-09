package de.reprise.spike

import de.reprise.spike.scene.CoreShape
import de.reprise.spike.scene.Transient
import kotlin.math.abs
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class NowPlayingBurstTest {
    @Test
    fun burst_geometry_matches_the_full_screen_specification() {
        assertEquals(112, NowPlayingBurstSpec.wedgeCount)
        assertEquals(168, NowPlayingBurstSpec.coronaStrokeCount)
        assertEquals(0.47f, NowPlayingBurstSpec.centerHeightFraction)
        assertEquals(86f, NowPlayingBurstSpec.coronaBaseRadiusDp)
        assertEquals(26f, NowPlayingBurstSpec.coronaBassRadiusDp)
        assertEquals(16f, NowPlayingBurstSpec.coronaBaseLengthDp)
        assertEquals(62f, NowPlayingBurstSpec.coronaBandLengthDp)
        assertEquals(2.1f, NowPlayingBurstSpec.coronaStrokeWidthDp)
        assertEquals(78f, NowPlayingBurstSpec.coreBaseRadiusDp)
    }

    @Test
    fun wedge_and_corona_mapping_stays_in_range_and_visits_every_band() {
        for (count in listOf(NowPlayingBurstSpec.wedgeCount, NowPlayingBurstSpec.coronaStrokeCount)) {
            val mapped = (0 until count).map { burstBandIndex(it, count, 24) }
            assertEquals((0 until 24).toSet(), mapped.toSet())
            assertTrue(mapped.all { it in 0 until 24 })
        }
    }

    @Test
    fun a_transient_projects_exactly_one_hot_ray_or_none() {
        assertNull(burstHotRay(null, 24))

        val ray = burstHotRay(Transient(bandIndex = 7, excess = 0.42f), 24)

        assertEquals(7, ray?.bandIndex)
        assertEquals(105f, ray?.angleDegrees)
        assertEquals(0.42f, ray?.excess)
    }

    @Test
    fun core_outline_uses_the_track_shape_and_bass_without_rerolling() {
        val shape = CoreShape("A track", "An artist")
        val first = burstCoreRadii(shape, bass = 0.6f, pointCount = 168)
        val second = burstCoreRadii(shape, bass = 0.6f, pointCount = 168)

        assertEquals(first.toList(), second.toList())
        assertTrue(first.max() - first.min() > 4f)
        assertTrue(first.all { it > 60f })
    }

    @Test
    fun bloom_is_quarter_resolution_squared_and_bounded_by_level() {
        assertEquals(BloomSize(270, 585), burstBloomSize(1080, 2340))
        assertEquals(6f, burstBloomBlurDp(0f))
        assertEquals(22f, burstBloomBlurDp(1f))
        assertTrue(abs(burstBloomOpacity(0.5f) - 0.25f) < 0.0001f)
    }

    @Test
    fun renderer_contains_no_independent_animation_clock() {
        val source = java.io.File("src/main/java/de/reprise/spike/NowPlayingBurst.kt").readText()

        assertTrue("the burst may only consume scene state", "withFrameNanos" !in source)
        assertTrue("the burst may not loop independently", "infiniteRepeatable" !in source)
        assertTrue("the burst may not read system time", "currentTimeMillis" !in source)
    }
}
