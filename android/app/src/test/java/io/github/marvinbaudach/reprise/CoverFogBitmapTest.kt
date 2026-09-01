package io.github.marvinbaudach.reprise

import android.graphics.Bitmap
import android.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
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
class CoverFogBitmapTest {
    @Test
    fun partially_faded_rim_keeps_the_source_colour() {
        val sourceColour = Color.rgb(240, 64, 16)
        val source = Bitmap.createBitmap(32, 32, Bitmap.Config.ARGB_8888).apply {
            eraseColor(sourceColour)
        }

        val texture = prepareFogTexture(source, Color.MAGENTA)

        listOf(texture).forEach { bitmap ->
            val y = bitmap.height / 2
            val rimX = (bitmap.width / 2 until bitmap.width).first { x ->
                Color.alpha(bitmap.getPixel(x, y)) in PARTIAL_ALPHA_RANGE
            }
            val rim = bitmap.getPixel(rimX, y)

            assertChannelClose("red", Color.red(sourceColour), Color.red(rim))
            assertChannelClose("green", Color.green(sourceColour), Color.green(rim))
            assertChannelClose("blue", Color.blue(sourceColour), Color.blue(rim))
        }
    }

    @Test
    fun blurred_fog_dissolves_before_the_texture_edge() {
        val source = Bitmap.createBitmap(32, 32, Bitmap.Config.ARGB_8888).apply {
            eraseColor(Color.WHITE)
        }

        val texture = prepareFogTexture(source, Color.MAGENTA)

        listOf(texture).forEach { bitmap ->
            val last = bitmap.width - 1
            val secondLast = last - 1
            for (coordinate in 0 until bitmap.width) {
                assertEquals(0, Color.alpha(bitmap.getPixel(coordinate, 0)))
                assertEquals(0, Color.alpha(bitmap.getPixel(coordinate, 1)))
                assertEquals(0, Color.alpha(bitmap.getPixel(coordinate, secondLast)))
                assertEquals(0, Color.alpha(bitmap.getPixel(coordinate, last)))
                assertEquals(0, Color.alpha(bitmap.getPixel(0, coordinate)))
                assertEquals(0, Color.alpha(bitmap.getPixel(1, coordinate)))
                assertEquals(0, Color.alpha(bitmap.getPixel(secondLast, coordinate)))
                assertEquals(0, Color.alpha(bitmap.getPixel(last, coordinate)))
            }
            listOf(0 to 0, last to 0, 0 to last, last to last).forEach { (x, y) ->
                assertEquals(0, Color.alpha(bitmap.getPixel(x, y)))
            }

            val centre = bitmap.width / 2
            val axisAlpha = (centre..last).map { x -> Color.alpha(bitmap.getPixel(x, centre)) }
            axisAlpha.zipWithNext().forEach { (inner, outer) ->
                assertTrue("axis alpha must fade toward the edge: $inner then $outer", outer <= inner)
            }
        }
    }

    /**
     * One cover, one 256 px disc and one palette — and the palette answers the
     * two halves separately.
     *
     * The fixture is white on the left and black on the right, so a palette
     * that read the cover as a single average would hand back four equal
     * colours. Quadrant means are what make the film iridescent, so the test is
     * that the left pair and the right pair disagree.
     */
    @Test
    fun artwork_is_cropped_once_into_a_disc_and_read_once_into_a_palette() {
        val source = Bitmap.createBitmap(8, 4, Bitmap.Config.ARGB_8888)
        for (y in 0 until source.height) {
            for (x in 0 until source.width) {
                source.setPixel(x, y, if (x < 4) Color.WHITE else Color.BLACK)
            }
        }

        val fog = prepareCoverFogBitmap(source, Color.MAGENTA)

        assertEquals(256, fog.disc.width)
        assertEquals(256, fog.disc.height)
        assertEquals(6, fog.palette.clouds.size)
        assertTrue(
            "the white half must read brighter than the black half",
            fog.palette.clouds[0].red > fog.palette.clouds[1].red,
        )
    }

    @Test
    fun shimmer_mask_has_a_solid_core_and_falls_monotonically_to_clear() {
        assertEquals(1f, shimmerMaskAlpha(0f), 0f)
        assertEquals(1f, shimmerMaskAlpha(0.12f), 0f)
        assertEquals(0f, shimmerMaskAlpha(0.68f), 0f)
        assertEquals(0f, shimmerMaskAlpha(1f), 0f)

        val falloff = (12..68).map { percent -> shimmerMaskAlpha(percent / 100f) }
        falloff.zipWithNext().forEach { (inner, outer) ->
            assertTrue("mask alpha must fall toward the edge: $inner then $outer", outer <= inner)
        }
    }

    @Test
    fun shimmer_disc_is_a_256_pixel_texture_baked_into_the_cached_fog() {
        val source = Bitmap.createBitmap(16, 16, Bitmap.Config.ARGB_8888).apply {
            eraseColor(Color.WHITE)
        }
        val artwork = source.asImageBitmap()
        val cache = ArtworkCache()
        val prepared = prepareCoverFogBitmap(source, Color.MAGENTA)

        cache.putFog(artwork, prepared)

        val cached = cache.fog(artwork)
        assertSame(prepared, cached)
        assertSame(prepared.disc, cached?.disc)
        assertEquals(256, prepared.disc.width)
        assertEquals(256, prepared.disc.height)
        assertEquals(255, Color.alpha(prepared.disc.getPixel(128, 128)))
        assertEquals(0, Color.alpha(prepared.disc.getPixel(128 + (256 * 0.34f).toInt(), 128)))
    }

    @Test
    fun greyscale_artwork_stays_greyscale_in_the_fog_texture() {
        val source = Bitmap.createBitmap(5, 5, Bitmap.Config.ARGB_8888)
        for (y in 0 until source.height) {
            for (x in 0 until source.width) {
                val value = (x + y) * 24
                source.setPixel(x, y, Color.rgb(value, value, value))
            }
        }

        val texture = prepareFogTexture(source, Color.MAGENTA)

        listOf(texture).forEach { bitmap ->
            for (coordinate in listOf(32, 128, 224)) {
                val pixel = bitmap.getPixel(coordinate, coordinate)
                assertEquals(Color.red(pixel), Color.green(pixel))
                assertEquals(Color.green(pixel), Color.blue(pixel))
            }
        }
    }

    @Test
    fun missing_artwork_uses_only_the_app_accent() {
        val accent = Color.rgb(18, 91, 204)

        val texture = prepareFogTexture(null, accent)

        listOf(texture).forEach { bitmap ->
            val pixel = bitmap.getPixel(128, 128)
            assertTrue(Color.red(pixel) in 17..19)
            assertTrue(Color.green(pixel) in 90..92)
            assertTrue(Color.blue(pixel) in 203..205)
        }
    }

    private fun assertChannelClose(channel: String, expected: Int, actual: Int) {
        assertTrue(
            "$channel channel $actual must stay within $RGB_TOLERANCE of source $expected",
            abs(actual - expected) <= RGB_TOLERANCE,
        )
    }

    private companion object {
        val PARTIAL_ALPHA_RANGE = 48..80
        const val RGB_TOLERANCE = 3
    }
}
