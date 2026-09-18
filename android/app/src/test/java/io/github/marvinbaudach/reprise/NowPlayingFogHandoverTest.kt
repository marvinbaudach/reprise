package io.github.marvinbaudach.reprise

import android.graphics.Bitmap
import android.graphics.Color
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [26])
class NowPlayingFogHandoverTest {
    @Test
    fun the_first_fog_has_nothing_outgoing_and_arrives_at_full_strength() {
        val fogA = fogOf(Color.BLACK)

        val handover = fogHandover(FogHandover.EMPTY, fogA, arrival = 1f)

        assertNull(handover.outgoing)
        assertSame(fogA, handover.incoming)
        assertEquals(1f, handover.incomingDiscAlpha(0f), 0f)
    }

    @Test
    fun a_plain_handover_at_rest_fades_the_old_fog_out_from_full_alpha() {
        val fogA = fogOf(Color.BLACK)
        val fogB = fogOf(Color.WHITE)
        val first = fogHandover(FogHandover.EMPTY, fogA, arrival = 1f)

        val handover = fogHandover(first, fogB, arrival = 1f)

        assertSame(fogA, handover.outgoing)
        assertEquals(1f, handover.outgoingAlpha, 0f)
        assertPaletteEquals(
            fogA.palette.blendedTo(fogB.palette, 0.5f),
            handover.paletteAt(0.5f),
        )
    }

    @Test
    fun a_restart_mid_fade_takes_over_the_mixture_the_screen_already_showed() {
        val fogA = fogOf(Color.BLACK)
        val fogB = fogOf(Color.WHITE)
        val fogC = fogOf(Color.rgb(210, 40, 40))
        val first = fogHandover(FogHandover.EMPTY, fogA, arrival = 1f)
        val plainHandover = fogHandover(first, fogB, arrival = 1f)

        val restarted = fogHandover(plainHandover, fogC, arrival = 0.4f)

        assertSame("the disc fading in becomes the disc fading out", fogB, restarted.outgoing)
        assertEquals(0.4f, restarted.outgoingAlpha, 0f)
        assertPaletteEquals(plainHandover.paletteAt(0.4f), restarted.outgoingPalette)
        assertTrue(
            "the older outgoing fog is dropped, never carried a third layer",
            restarted.outgoing !== fogA,
        )
    }

    @Test
    fun the_same_fog_object_again_returns_the_state_unchanged() {
        val fogA = fogOf(Color.BLACK)
        val fogB = fogOf(Color.WHITE)
        val first = fogHandover(FogHandover.EMPTY, fogA, arrival = 1f)
        val handover = fogHandover(first, fogB, arrival = 0.3f)

        val unchanged = fogHandover(handover, fogB, arrival = 0.7f)

        assertSame(handover, unchanged)
    }

    @Test
    fun disc_alphas_sum_to_one_at_every_point_of_a_plain_handover() {
        val fogA = fogOf(Color.BLACK)
        val fogB = fogOf(Color.WHITE)
        val first = fogHandover(FogHandover.EMPTY, fogA, arrival = 1f)
        val handover = fogHandover(first, fogB, arrival = 1f)

        listOf(0f, 0.25f, 0.5f, 1f).forEach { t ->
            assertEquals(
                "outgoing and incoming discs must sum to 1 at t=$t or the light dips",
                1f,
                handover.outgoingDiscAlpha(t) + handover.incomingDiscAlpha(t),
                1e-6f,
            )
        }
    }

    @Test
    fun continued_film_clock_reads_the_same_seconds_right_after_the_handover() {
        val offsets = continuedFogClocks(
            shownFilmSeconds = 100f,
            shownShimmerSeconds = 0.0,
            newFilmSeconds = 3f,
            newShimmerSeconds = 0.0,
        )

        assertEquals(97f, offsets.filmSeconds, 1e-6f)
        assertEquals(100f, 3f + offsets.filmSeconds, 1e-6f)
    }

    @Test
    fun continued_shimmer_clock_wraps_into_the_turn_and_reads_the_same_seconds() {
        val offsets = continuedFogClocks(
            shownFilmSeconds = 0f,
            shownShimmerSeconds = 59.5,
            newFilmSeconds = 0f,
            newShimmerSeconds = 2.0,
        )

        val displayed = (2.0 + offsets.shimmerSeconds).mod(SHIMMER_TURN_SECONDS)
        assertEquals(59.5, displayed, 1e-9)
    }

    @Test
    fun a_negative_shimmer_difference_wraps_into_zero_to_sixty() {
        val offsets = continuedFogClocks(
            shownFilmSeconds = 0f,
            shownShimmerSeconds = 2.0,
            newFilmSeconds = 0f,
            newShimmerSeconds = 59.5,
        )

        assertTrue(
            "a negative difference must wrap into [0, 60)",
            offsets.shimmerSeconds >= 0.0 && offsets.shimmerSeconds < SHIMMER_TURN_SECONDS,
        )
        val displayed = (59.5 + offsets.shimmerSeconds).mod(SHIMMER_TURN_SECONDS)
        assertEquals(2.0, displayed, 1e-9)
    }

    private fun assertPaletteEquals(expected: OilFilmPalette?, actual: OilFilmPalette?) {
        assertEquals(expected?.clouds, actual?.clouds)
    }

    private fun fogOf(colour: Int): CoverFogBitmap {
        val source = Bitmap.createBitmap(8, 8, Bitmap.Config.ARGB_8888).apply { eraseColor(colour) }
        return prepareCoverFogBitmap(source, Color.MAGENTA)
    }
}
