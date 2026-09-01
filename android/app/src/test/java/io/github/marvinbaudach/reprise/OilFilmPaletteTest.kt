package io.github.marvinbaudach.reprise

import android.graphics.Bitmap
import android.graphics.Color
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import kotlin.math.abs

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [26])
class OilFilmPaletteTest {
    /**
     * The reason the lift exists: a black cover has to produce a visible film.
     *
     * Death metal artwork is the worst case and it is not rare — a cover whose
     * quadrants all average under 20 would give six clouds of near black, and
     * six near-black clouds screen-blended over a near-black scene are nothing
     * at all. Every channel has to clear the floor.
     */
    @Test
    fun a_black_cover_still_produces_a_visible_film() {
        val quadrants = extractOilFilmQuadrants(solid(Color.BLACK))

        quadrants.forEach { colour ->
            assertTrue(
                "a black quadrant must be lifted clear of the ground: $colour",
                colour.red * 255f >= 39f,
            )
        }
    }

    /** And a white one must not wash out into a flat bright field. */
    @Test
    fun a_white_cover_is_held_below_a_flat_bright_field() {
        val quadrants = extractOilFilmQuadrants(solid(Color.WHITE))

        quadrants.forEach { colour ->
            assertTrue(
                "a white quadrant must stay under the ceiling: $colour",
                colour.red <= 211f / 255f,
            )
        }
    }

    /** The lift moves every channel the same way, so a cover keeps its hue. */
    @Test
    fun the_lift_raises_a_dark_cover_without_turning_it() {
        val quadrants = extractOilFilmQuadrants(solid(Color.rgb(20, 8, 4)))

        quadrants.forEach { colour ->
            assertTrue("red must stay the strongest channel", colour.red > colour.green)
            assertTrue("green must stay above blue", colour.green > colour.blue)
        }
    }

    /** Four corners of a cover are read as four different colours. */
    @Test
    fun the_four_quadrants_are_read_separately() {
        val source = Bitmap.createBitmap(16, 16, Bitmap.Config.ARGB_8888)
        for (y in 0 until 16) {
            for (x in 0 until 16) {
                val colour = when {
                    x < 8 && y < 8 -> Color.rgb(200, 20, 20)
                    x >= 8 && y < 8 -> Color.rgb(20, 200, 20)
                    x < 8 -> Color.rgb(20, 20, 200)
                    else -> Color.rgb(200, 200, 20)
                }
                source.setPixel(x, y, colour)
            }
        }

        val quadrants = extractOilFilmQuadrants(source)

        assertEquals(4, quadrants.size)
        assertTrue("top left is red", quadrants[0].red > quadrants[0].green)
        assertTrue("top right is green", quadrants[1].green > quadrants[1].red)
        assertTrue("bottom left is blue", quadrants[2].blue > quadrants[2].red)
        assertTrue("bottom right is yellow", quadrants[3].red > quadrants[3].blue)
    }

    /** Six clouds out of four colours, the last two mixed across the diagonals. */
    @Test
    fun four_colours_are_spread_into_six_across_the_diagonals() {
        val quadrants = listOf(
            androidx.compose.ui.graphics.Color(1f, 0f, 0f),
            androidx.compose.ui.graphics.Color(0f, 1f, 0f),
            androidx.compose.ui.graphics.Color(0f, 0f, 1f),
            androidx.compose.ui.graphics.Color(1f, 1f, 0f),
        )

        val palette = spreadOilFilmClouds(quadrants)

        assertEquals(6, palette.clouds.size)
        assertEquals(6, palette.brushes.size)
        // Fifth is first mixed with fourth: red and yellow, so red stays high
        // and blue stays out of it entirely.
        assertTrue("fifth cloud carries no blue", palette.clouds[4].blue < 0.05f)
        assertTrue("fifth cloud is warm", palette.clouds[4].red > palette.clouds[4].blue)
        // Sixth is second mixed with third: green and blue, no red.
        assertTrue("sixth cloud carries no red", palette.clouds[5].red < 0.25f)
        assertTrue("sixth cloud is cool", palette.clouds[5].blue > palette.clouds[5].red)
    }

    /** The grade the design applies as a filter is applied to the colours instead. */
    @Test
    fun the_film_is_graded_rather_than_filtered() {
        val flat = androidx.compose.ui.graphics.Color(0.5f, 0.4f, 0.45f)
        val palette = spreadOilFilmClouds(List(4) { flat })

        val graded = palette.clouds.first()
        val beforeSpread = maxOf(flat.red, flat.green, flat.blue) -
            minOf(flat.red, flat.green, flat.blue)
        val afterSpread = maxOf(graded.red, graded.green, graded.blue) -
            minOf(graded.red, graded.green, graded.blue)

        assertTrue(
            "saturate must widen the channel spread: $beforeSpread then $afterSpread",
            afterSpread > beforeSpread,
        )
    }

    /** Behind the spectrum the film reads the visualizer's own cyan-to-magenta ramp. */
    @Test
    fun the_visualizer_palette_runs_cyan_to_magenta_and_stays_dim() {
        val ramp = visualizerRampQuadrants()

        assertEquals(4, ramp.size)
        assertTrue("the ramp opens on cyan", ramp.first().blue > ramp.first().red)
        assertTrue("the ramp closes on magenta", ramp.last().red > ramp.last().green)
        ramp.forEach { colour ->
            val brightest = maxOf(colour.red, colour.green, colour.blue)
            assertTrue("the ramp must stay a haze, not a light: $brightest", brightest < 0.72f)
        }
    }

    /** Nothing to read is not a crash; it is a neutral film at the floor. */
    @Test
    fun a_missing_cover_falls_back_to_a_neutral_film() {
        val quadrants = extractOilFilmQuadrants(null)

        assertEquals(4, quadrants.size)
        quadrants.forEach { colour ->
            assertEquals(colour.red, colour.green, 1e-6f)
            assertEquals(colour.green, colour.blue, 1e-6f)
        }
    }

    /** A cross-fade hands back the ends themselves, and something between in between. */
    @Test
    fun a_cross_fade_returns_its_ends_untouched() {
        val cover = spreadOilFilmClouds(extractOilFilmQuadrants(solid(Color.rgb(200, 40, 40))))
        val ramp = spreadOilFilmClouds(visualizerRampQuadrants())

        assertSame(cover, cover.blendedTo(ramp, 0f))
        assertSame(ramp, cover.blendedTo(ramp, 1f))
        val middle = cover.blendedTo(ramp, 0.5f).clouds.first()
        val expected = (cover.clouds.first().blue + ramp.clouds.first().blue) / 2f
        // Compose packs an sRGB colour at eight bits a channel, so "between"
        // is only ever exact to a 255th.
        assertTrue("halfway must sit between the two", abs(middle.blue - expected) < 0.005f)
    }

    /**
     * The artist page spends alpha on this, so it has to separate the two ends.
     *
     * A white cover and a black one both come out of the lift with a floor and a
     * ceiling on them, and a caller that dims the film by this number needs the
     * gap between them to survive that clamping — otherwise every artwork asks
     * for the same alpha and the reduction does nothing.
     */
    @Test
    fun a_bright_cover_reads_brighter_than_a_dark_one() {
        val bright = spreadOilFilmClouds(extractOilFilmQuadrants(solid(Color.WHITE)))
        val dark = spreadOilFilmClouds(extractOilFilmQuadrants(solid(Color.BLACK)))

        assertTrue(
            "a white cover must carry more light than a black one",
            bright.meanLuminance > dark.meanLuminance + 0.3f,
        )
        assertTrue("luminance is a fraction", dark.meanLuminance >= 0f)
        assertTrue("luminance is a fraction", bright.meanLuminance <= 1f)
    }

    private fun solid(colour: Int): Bitmap =
        Bitmap.createBitmap(16, 16, Bitmap.Config.ARGB_8888).apply { eraseColor(colour) }
}
