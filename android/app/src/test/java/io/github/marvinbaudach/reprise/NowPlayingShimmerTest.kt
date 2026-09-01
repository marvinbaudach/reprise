package io.github.marvinbaudach.reprise

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

    /**
     * Replaces `shimmer_opacity_rises_with_pressure_and_swell_and_clamps_each_input`.
     *
     * The disc answered the kick with a term of its own, in step with the fog
     * behind it, so both brightened together on every beat. The kick is gone
     * from here for the same reason it is gone from the fog, and its weight was
     * folded into the level rather than dropped: rest and peak are the numbers
     * the old rule pinned, unchanged.
     */
    @Test
    fun shimmer_opacity_rises_with_the_level_alone_and_clamps_it() {
        val rest = NowPlayingShimmerSpec.alpha(swell = 0f, opacity = 1f)
        val peak = NowPlayingShimmerSpec.alpha(swell = 0.70f, opacity = 1f)

        assertEquals(0.113_333f, rest, FLOAT_TOLERANCE)
        assertEquals(0.213_333f, peak, FLOAT_TOLERANCE)
        assertTrue(peak > rest)
        assertEquals(rest, NowPlayingShimmerSpec.alpha(-1f, 1f), FLOAT_TOLERANCE)
        assertEquals(peak, NowPlayingShimmerSpec.alpha(2f, 1f), FLOAT_TOLERANCE)
        assertEquals(0f, NowPlayingShimmerSpec.alpha(1f, -1f), FLOAT_TOLERANCE)
        assertEquals(peak, NowPlayingShimmerSpec.alpha(1f, 2f), FLOAT_TOLERANCE)
    }

    @Test
    fun `the disc on a bare surface keeps the desktop alphas`() {
        val scale = NowPlayingShimmerSpec.ON_BARE_SURFACE_SCALE
        val rest = NowPlayingShimmerSpec.alpha(swell = 0f, opacity = 1f, scale = scale)
        val peak = NowPlayingShimmerSpec.alpha(swell = 0.70f, opacity = 1f, scale = scale)

        assertEquals(0.34f, rest, FLOAT_TOLERANCE)
        assertEquals(0.64f, peak, FLOAT_TOLERANCE)
        assertEquals(
            rest,
            NowPlayingShimmerSpec.alpha(0f, 1f, NowPlayingShimmerSpec.OVER_FOG_SCALE) * 3f,
            FLOAT_TOLERANCE,
        )
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
