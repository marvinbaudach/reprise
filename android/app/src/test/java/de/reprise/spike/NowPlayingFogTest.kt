package de.reprise.spike

import de.reprise.spike.scene.FogDrive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class NowPlayingFogTest {
    @Test
    fun fog_alpha_answers_the_measured_p5_to_p95_energy_step() {
        val quietWide = NowPlayingFogSpec.wideAlpha(swell = 0.13082694f, opacity = 1f)
        val loudWide = NowPlayingFogSpec.wideAlpha(swell = 0.64443964f, opacity = 1f)
        val quietTight = NowPlayingFogSpec.tightAlpha(swell = 0.13082694f, opacity = 1f)
        val loudTight = NowPlayingFogSpec.tightAlpha(swell = 0.64443964f, opacity = 1f)

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
                NowPlayingFogSpec.wideAlpha(level, opacity = 1f).toRawBits(),
            )
            assertEquals(
                0.55f.toRawBits(),
                NowPlayingFogSpec.tightAlpha(level, opacity = 1f).toRawBits(),
            )
        }

        assertEquals(
            0.8826339f,
            NowPlayingFogSpec.wideAlpha(swell = 0.66f, opacity = 1f),
            FLOAT_TOLERANCE,
        )
        assertEquals(
            0.5208923f,
            NowPlayingFogSpec.tightAlpha(swell = 0.66f, opacity = 1f),
            FLOAT_TOLERANCE,
        )
    }

    /**
     * Replaces `a_kick_at_constant_swell_brightens_both_layers_without_pumping_their_size`.
     *
     * That rule was deliberate once: the kick was what made the haze legible as
     * a response to the music at all. It is withdrawn because the layer it
     * brightens is most of the screen, and one brightening per beat is four a
     * second on the music this app is built around — past the three-per-second
     * line WCAG 2.3.1 draws for photosensitive seizures. What replaces it is
     * not "less kick" but "no kick": the level in [FogDrive] carries the whole
     * response now, and it is rate-capped so no signal at any tempo can flash
     * the screen.
     */
    @Test
    fun no_signal_at_any_tempo_can_flash_the_fog() {
        val onBeat = 0.9f
        val offBeat = 0.1f
        // Four kicks a second: the tempo the old response strobed at.
        val secondsPerHalfBeat = 1f / 8f

        var drive = offBeat
        var lowest = Float.MAX_VALUE
        var highest = -Float.MAX_VALUE
        repeat(80) { half ->
            drive = FogDrive.step(drive, if (half % 2 == 0) onBeat else offBeat, secondsPerHalfBeat)
            val alpha = NowPlayingFogSpec.wideAlpha(drive, opacity = 1f)
            lowest = minOf(lowest, alpha)
            highest = maxOf(highest, alpha)
        }

        // WCAG treats a swing below 10% of relative luminance as no flash at
        // all. The fog's own peak is the whole budget it can spend, so the
        // swing is measured against that rather than against the screen.
        assertTrue(
            "a double-kick must not swing the fog by a tenth of its range, got ${highest - lowest}",
            highest - lowest < 0.10f * NowPlayingFogSpec.wideOpacity,
        )
    }

    @Test
    fun a_long_loud_passage_still_reaches_the_top_of_the_range() {
        var drive = 0f
        // The cap trades immediacy for safety, and this is the price: the haze
        // needs seconds, not beats. It must still get there, or the response is
        // gone rather than slowed.
        repeat(120) { drive = FogDrive.step(drive, 1f, 1f / 60f) }

        assertEquals(
            NowPlayingFogSpec.wideOpacity.toRawBits(),
            NowPlayingFogSpec.wideAlpha(drive, opacity = 1f).toRawBits(),
        )
        assertTrue(
            "two seconds must be enough to cross the range",
            FogDrive.FULL_RANGE_SECONDS <= 2f,
        )
    }

    @Test
    fun swell_at_full_keeps_the_existing_alpha_ceiling() {
        assertEquals(
            NowPlayingFogSpec.wideOpacity.toRawBits(),
            NowPlayingFogSpec.wideAlpha(swell = 1f, opacity = 1f).toRawBits(),
        )
        assertEquals(
            NowPlayingFogSpec.tightOpacity.toRawBits(),
            NowPlayingFogSpec.tightAlpha(swell = 1f, opacity = 1f).toRawBits(),
        )
    }

    @Test
    fun swell_clamps_instead_of_extrapolating() {
        fun assertClamped(alpha: (Float, Float) -> Float) {
            assertEquals(alpha(0f, 1f).toRawBits(), alpha(-1f, 1f).toRawBits())
            assertEquals(alpha(1f, 1f).toRawBits(), alpha(2f, 1f).toRawBits())
        }
        assertClamped { swell, opacity -> NowPlayingFogSpec.wideAlpha(swell, opacity) }
        assertClamped { swell, opacity -> NowPlayingFogSpec.tightAlpha(swell, opacity) }
    }

    @Test
    fun quiet_fog_keeps_the_accepted_atmosphere_floor() {
        val wideFloor = NowPlayingFogSpec.wideAlpha(swell = 0f, opacity = 1f)
        val tightFloor = NowPlayingFogSpec.tightAlpha(swell = 0f, opacity = 1f)

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

    /**
     * The kick must not find its way back in through a caller.
     *
     * Every other rule here is about the numbers the spec returns, and none of
     * them would notice a scene that went back to passing the detector's kick
     * as the swell — the arithmetic would stay correct and the screen would
     * strobe again.
     */
    @Test
    fun no_drawing_path_feeds_the_kick_into_the_haze() {
        val source = listOf(
            "src/main/java/de/reprise/spike/NowPlayingFog.kt",
            "src/main/java/de/reprise/spike/NowPlayingShimmer.kt",
            "src/main/java/de/reprise/spike/NowPlayingScene.kt",
        ).joinToString("\n") { java.io.File(it).readText() }

        assertTrue(
            "the fog and shimmer must not read bassPressure",
            "bassPressure = state.bassPressure" !in source,
        )
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
