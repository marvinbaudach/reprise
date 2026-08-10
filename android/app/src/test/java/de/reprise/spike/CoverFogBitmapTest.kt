package de.reprise.spike

import android.graphics.Bitmap
import android.graphics.Color
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [26])
class CoverFogBitmapTest {
    @Test
    fun blurred_fog_dissolves_before_the_texture_edge() {
        val source = Bitmap.createBitmap(32, 32, Bitmap.Config.ARGB_8888).apply {
            eraseColor(Color.WHITE)
        }

        val fog = prepareCoverFogBitmap(source, Color.MAGENTA)

        listOf(fog.wide, fog.tight).forEach { bitmap ->
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

    @Test
    fun artwork_is_cropped_once_into_two_distinct_256_pixel_preblurred_layers() {
        val source = Bitmap.createBitmap(8, 4, Bitmap.Config.ARGB_8888)
        for (y in 0 until source.height) {
            for (x in 0 until source.width) {
                source.setPixel(x, y, if (x < 4) Color.WHITE else Color.BLACK)
            }
        }

        val fog = prepareCoverFogBitmap(source, Color.MAGENTA)

        assertEquals(256, fog.wide.width)
        assertEquals(256, fog.wide.height)
        assertEquals(256, fog.tight.width)
        assertEquals(256, fog.tight.height)
        assertNotEquals(fog.wide.getPixel(128, 128), fog.tight.getPixel(128, 128))
    }

    @Test
    fun greyscale_artwork_stays_greyscale_in_both_fog_layers() {
        val source = Bitmap.createBitmap(5, 5, Bitmap.Config.ARGB_8888)
        for (y in 0 until source.height) {
            for (x in 0 until source.width) {
                val value = (x + y) * 24
                source.setPixel(x, y, Color.rgb(value, value, value))
            }
        }

        val fog = prepareCoverFogBitmap(source, Color.MAGENTA)

        listOf(fog.wide, fog.tight).forEach { bitmap ->
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

        val fog = prepareCoverFogBitmap(null, accent)

        listOf(fog.wide, fog.tight).forEach { bitmap ->
            val pixel = bitmap.getPixel(128, 128)
            assertTrue(Color.red(pixel) in 17..19)
            assertTrue(Color.green(pixel) in 90..92)
            assertTrue(Color.blue(pixel) in 203..205)
        }
    }
}
