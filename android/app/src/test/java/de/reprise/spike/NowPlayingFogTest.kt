package de.reprise.spike

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class NowPlayingFogTest {
    @Test
    fun fog_alpha_answers_the_measured_p5_to_p95_energy_step() {
        val quietWide = NowPlayingFogSpec.wideAlpha(fogLevel = 0.1308f, opacity = 1f)
        val loudWide = NowPlayingFogSpec.wideAlpha(fogLevel = 0.6444f, opacity = 1f)
        val quietTight = NowPlayingFogSpec.tightAlpha(fogLevel = 0.1308f, opacity = 1f)
        val loudTight = NowPlayingFogSpec.tightAlpha(fogLevel = 0.6444f, opacity = 1f)

        assertTrue(
            "measured p5-to-p95 energy must lift effective wide alpha by at least 40%",
            loudWide >= quietWide * 1.40f,
        )
        assertTrue(
            "measured p5-to-p95 energy must lift effective tight alpha by at least 90%",
            loudTight >= quietTight * 1.90f,
        )
    }

    @Test
    fun fog_alpha_never_exceeds_the_signed_off_peak() {
        listOf(0.70f, 1f).forEach { fogLevel ->
            assertEquals(
                0.92f.toRawBits(),
                NowPlayingFogSpec.wideAlpha(fogLevel, opacity = 1f).toRawBits(),
            )
            assertEquals(
                0.55f.toRawBits(),
                NowPlayingFogSpec.tightAlpha(fogLevel, opacity = 1f).toRawBits(),
            )
        }

        assertEquals(
            0.89848614f,
            NowPlayingFogSpec.wideAlpha(fogLevel = 0.66f, opacity = 1f),
            FLOAT_TOLERANCE,
        )
        assertEquals(
            0.5296923f,
            NowPlayingFogSpec.tightAlpha(fogLevel = 0.66f, opacity = 1f),
            FLOAT_TOLERANCE,
        )
    }

    @Test
    fun quiet_fog_keeps_the_accepted_atmosphere_floor() {
        val wideFloor = NowPlayingFogSpec.wideAlpha(fogLevel = 0f, opacity = 1f)
        val tightFloor = NowPlayingFogSpec.tightAlpha(fogLevel = 0f, opacity = 1f)

        assertEquals(0.62f, wideFloor / NowPlayingFogSpec.wideOpacity, FLOAT_TOLERANCE)
        assertEquals(0.40f, tightFloor / NowPlayingFogSpec.tightOpacity, FLOAT_TOLERANCE)
        assertTrue(
            "the dominant wide fog must retain at least 55% of its peak in silence",
            wideFloor >= NowPlayingFogSpec.wideOpacity * 0.55f,
        )
    }

    @Test
    fun fog_alpha_is_monotonically_non_decreasing_with_energy() {
        val levels = listOf(-1f, 0f, 0.05f, 0.1308f, 0.66f, 0.70f, 1f, 2f)

        assertMonotonic(levels.map { NowPlayingFogSpec.wideAlpha(it, opacity = 1f) })
        assertMonotonic(levels.map { NowPlayingFogSpec.tightAlpha(it, opacity = 1f) })
    }

    @Test
    fun fog_layers_keep_the_specified_geometry_blend_and_counter_rotation() {
        assertEquals(620f, NowPlayingFogSpec.wideSizeDp)
        assertEquals(470f, NowPlayingFogSpec.tightSizeDp)
        assertEquals(0.92f, NowPlayingFogSpec.wideOpacity)
        assertEquals(0.55f, NowPlayingFogSpec.tightOpacity)
        assertEquals(0.9f, NowPlayingFogSpec.wideAngleFactor)
        assertEquals(-0.6f, NowPlayingFogSpec.tightAngleFactor)
        assertTrue(NowPlayingFogSpec.tightUsesScreenBlend)
    }

    @Test
    fun the_title_scrim_covers_the_title_rows_without_outdarkening_the_edges() {
        // The rows the scene actually draws: title block top is 156 dp under
        // the cover centre, a two-line title plus the artist line ends by 246.
        assertTrue(
            "the scrim must be at full strength before the title starts",
            NowPlayingFogSpec.titleScrimSolidTopDp <= 156f,
        )
        assertTrue(
            "the scrim must still be at full strength where the artist line ends",
            NowPlayingFogSpec.titleScrimSolidBottomDp >= 246f,
        )
        assertTrue(
            "the scrim needs room to fade in above the rows",
            NowPlayingFogSpec.titleScrimFadeTopDp < NowPlayingFogSpec.titleScrimSolidTopDp,
        )
        assertTrue(
            "the scrim needs room to fade out below the rows",
            NowPlayingFogSpec.titleScrimFadeBottomDp > NowPlayingFogSpec.titleScrimSolidBottomDp,
        )
        // A dark cover must not end up with the middle of the screen darker
        // than its own edges, which would read as a hole rather than a scrim.
        assertTrue(
            "the title scrim must stay lighter than the top and bottom scrims",
            NowPlayingFogSpec.titleScrimAlpha < 0.72f,
        )
    }

    @Test
    fun cover_fog_preparation_never_depends_on_api_31_blur() {
        val source = java.io.File(
            "src/main/java/de/reprise/spike/NowPlayingFog.kt",
        ).readText() + java.io.File(
            "src/main/java/de/reprise/spike/CoverFogBitmap.kt",
        ).readText()

        assertTrue("Modifier.blur must not enter the Now Playing fog", "Modifier.blur" !in source)
        assertTrue("RenderEffect would violate minSdk 26", "RenderEffect" !in source)
    }

    private fun assertMonotonic(values: List<Float>) {
        values.zipWithNext().forEach { (before, after) ->
            assertTrue("$after must not be below $before", after >= before)
        }
    }

    private companion object {
        const val FLOAT_TOLERANCE = 0.000_001f
    }
}
