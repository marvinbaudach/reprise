package de.reprise.spike

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class NowPlayingFogTest {
    @Test
    fun fog_alpha_answers_the_measured_p5_to_p95_energy_step() {
        val quietWide = NowPlayingFogSpec.wideAlpha(
            swell = 0.13082694f,
            bassPressure = 0.00002848626f,
            opacity = 1f,
        )
        val loudWide = NowPlayingFogSpec.wideAlpha(
            swell = 0.64443964f,
            bassPressure = 0.6560269f,
            opacity = 1f,
        )
        val quietTight = NowPlayingFogSpec.tightAlpha(
            swell = 0.13082694f,
            bassPressure = 0.00002848626f,
            opacity = 1f,
        )
        val loudTight = NowPlayingFogSpec.tightAlpha(
            swell = 0.64443964f,
            bassPressure = 0.6560269f,
            opacity = 1f,
        )

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
        listOf(0.70f, 1f).forEach { level ->
            assertEquals(
                0.92f.toRawBits(),
                NowPlayingFogSpec.wideAlpha(level, level, opacity = 1f).toRawBits(),
            )
            assertEquals(
                0.55f.toRawBits(),
                NowPlayingFogSpec.tightAlpha(level, level, opacity = 1f).toRawBits(),
            )
        }

        assertEquals(
            0.883915f,
            NowPlayingFogSpec.wideAlpha(swell = 0.66f, bassPressure = 0.66f, opacity = 1f),
            FLOAT_TOLERANCE,
        )
        assertEquals(
            0.5218903f,
            NowPlayingFogSpec.tightAlpha(swell = 0.66f, bassPressure = 0.66f, opacity = 1f),
            FLOAT_TOLERANCE,
        )
    }

    @Test
    fun a_kick_at_constant_swell_brightens_both_layers_without_pumping_their_size() {
        val swell = 0.42f
        val quietWide = NowPlayingFogSpec.wideAlpha(swell, bassPressure = 0f, opacity = 1f)
        val kickWide = NowPlayingFogSpec.wideAlpha(swell, bassPressure = 0.65f, opacity = 1f)
        val quietTight = NowPlayingFogSpec.tightAlpha(swell, bassPressure = 0f, opacity = 1f)
        val kickTight = NowPlayingFogSpec.tightAlpha(swell, bassPressure = 0.65f, opacity = 1f)
        val sizeBefore = NowPlayingFogSpec.breathingSize(NowPlayingFogSpec.wideSizeDp, swell)
        val sizeAfter = NowPlayingFogSpec.breathingSize(NowPlayingFogSpec.wideSizeDp, swell)

        assertTrue(kickWide - quietWide > ALPHA_QUANTIZATION_FLOOR)
        assertTrue(kickTight - quietTight > ALPHA_QUANTIZATION_FLOOR)
        assertEquals(sizeBefore.toRawBits(), sizeAfter.toRawBits())
    }

    @Test
    fun pressure_and_swell_at_full_keep_the_existing_alpha_ceiling() {
        assertEquals(
            NowPlayingFogSpec.wideOpacity.toRawBits(),
            NowPlayingFogSpec.wideAlpha(swell = 1f, bassPressure = 1f, opacity = 1f).toRawBits(),
        )
        assertEquals(
            NowPlayingFogSpec.tightOpacity.toRawBits(),
            NowPlayingFogSpec.tightAlpha(swell = 1f, bassPressure = 1f, opacity = 1f).toRawBits(),
        )
    }

    @Test
    fun pressure_and_swell_clamp_instead_of_extrapolating() {
        fun assertClamped(alpha: (Float, Float, Float) -> Float) {
            assertEquals(alpha(0f, 0.35f, 1f).toRawBits(), alpha(-1f, 0.35f, 1f).toRawBits())
            assertEquals(alpha(1f, 0.35f, 1f).toRawBits(), alpha(2f, 0.35f, 1f).toRawBits())
            assertEquals(alpha(0.35f, 0f, 1f).toRawBits(), alpha(0.35f, -1f, 1f).toRawBits())
            assertEquals(alpha(0.35f, 1f, 1f).toRawBits(), alpha(0.35f, 2f, 1f).toRawBits())
        }
        assertClamped { swell, pressure, opacity ->
            NowPlayingFogSpec.wideAlpha(swell, pressure, opacity)
        }
        assertClamped { swell, pressure, opacity ->
            NowPlayingFogSpec.tightAlpha(swell, pressure, opacity)
        }
    }

    @Test
    fun quiet_fog_keeps_the_accepted_atmosphere_floor() {
        val wideFloor = NowPlayingFogSpec.wideAlpha(swell = 0f, bassPressure = 0f, opacity = 1f)
        val tightFloor = NowPlayingFogSpec.tightAlpha(swell = 0f, bassPressure = 0f, opacity = 1f)

        // The floors were lowered on 2026-08-10: at 0.62 and 0.40 the user
        // could not tell loud from quiet at all, so the swing was widened
        // deliberately. What still must hold is that silence is atmosphere
        // rather than absence — the haze never disappears.
        assertEquals(0.34f, wideFloor / NowPlayingFogSpec.wideOpacity, FLOAT_TOLERANCE)
        assertEquals(0.14f, tightFloor / NowPlayingFogSpec.tightOpacity, FLOAT_TOLERANCE)
        assertTrue(
            "the dominant wide fog must stay visible in silence",
            wideFloor >= NowPlayingFogSpec.wideOpacity * 0.30f,
        )
    }

    @Test
    fun fog_alpha_is_monotonically_non_decreasing_with_energy() {
        val levels = listOf(-1f, 0f, 0.05f, 0.1308f, 0.66f, 0.70f, 1f, 2f)

        assertMonotonic(levels.map { NowPlayingFogSpec.wideAlpha(it, it, opacity = 1f) })
        assertMonotonic(levels.map { NowPlayingFogSpec.tightAlpha(it, it, opacity = 1f) })
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
        const val ALPHA_QUANTIZATION_FLOOR = 1f / 255f
    }
}
