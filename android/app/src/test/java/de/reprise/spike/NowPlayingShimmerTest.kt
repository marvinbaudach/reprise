package de.reprise.spike

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class NowPlayingShimmerTest {
    @Test
    fun shimmer_turns_once_per_minute_and_wraps_long_sessions() {
        assertEquals(0f, NowPlayingShimmerSpec.angleDegrees(0.0), FLOAT_TOLERANCE)
        assertEquals(180f, NowPlayingShimmerSpec.angleDegrees(30.0), FLOAT_TOLERANCE)
        assertEquals(0f, NowPlayingShimmerSpec.angleDegrees(60.0), FLOAT_TOLERANCE)

        val hundredYearsAndHalfATurn = 100.0 * 365.0 * 24.0 * 60.0 * 60.0 + 30.0
        assertEquals(
            180f,
            NowPlayingShimmerSpec.angleDegrees(hundredYearsAndHalfATurn),
            FLOAT_TOLERANCE,
        )
    }

    @Test
    fun shimmer_opacity_rises_with_pressure_and_swell_and_clamps_each_input() {
        val rest = NowPlayingShimmerSpec.alpha(swell = 0f, bassPressure = 0f, opacity = 1f)
        val pressure = NowPlayingShimmerSpec.alpha(
            swell = 0f,
            bassPressure = 0.70f,
            opacity = 1f,
        )
        val swell = NowPlayingShimmerSpec.alpha(
            swell = 0.70f,
            bassPressure = 0f,
            opacity = 1f,
        )
        val peak = NowPlayingShimmerSpec.alpha(
            swell = 0.70f,
            bassPressure = 0.70f,
            opacity = 1f,
        )

        assertEquals(0.113_333f, rest, FLOAT_TOLERANCE)
        assertEquals(0.160_000f, pressure, FLOAT_TOLERANCE)
        assertEquals(0.166_667f, swell, FLOAT_TOLERANCE)
        assertEquals(0.213_333f, peak, FLOAT_TOLERANCE)
        assertTrue(pressure > rest)
        assertTrue(swell > rest)
        assertEquals(rest, NowPlayingShimmerSpec.alpha(-1f, -1f, 1f), FLOAT_TOLERANCE)
        assertEquals(peak, NowPlayingShimmerSpec.alpha(2f, 2f, 1f), FLOAT_TOLERANCE)
        assertEquals(0f, NowPlayingShimmerSpec.alpha(1f, 1f, -1f), FLOAT_TOLERANCE)
        assertEquals(peak, NowPlayingShimmerSpec.alpha(1f, 1f, 2f), FLOAT_TOLERANCE)
    }

    @Test
    fun shimmer_diameter_keeps_the_desktop_520_to_168_cover_ratio() {
        val coverDiameterDp = 272f
        val shimmerDiameterDp = NowPlayingShimmerSpec.diameterDp(coverDiameterDp)

        assertEquals(520f / 168f, shimmerDiameterDp / coverDiameterDp, FLOAT_TOLERANCE)
        assertEquals(841.9048f, shimmerDiameterDp, FLOAT_TOLERANCE)
    }

    private companion object {
        const val FLOAT_TOLERANCE = 0.000_1f
    }
}
