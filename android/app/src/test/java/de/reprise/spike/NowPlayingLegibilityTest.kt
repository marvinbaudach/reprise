package de.reprise.spike

import android.graphics.Bitmap
import android.graphics.Color
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Canvas
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.CanvasDrawScope
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * Wave D check 5: a very bright cover must not swallow the title.
 *
 * The emulator run of 2026-08-09 measured 2.32:1 for the title and 1.28:1 for
 * the artist line against the brightest analysed cover in the library — a
 * near-white image whose tenth percentile is already 204/255. This test holds
 * the scene to the readable side of that measurement, using the same worst
 * case and the same WCAG relative-luminance arithmetic.
 *
 * The type is fixed here on purpose. The brief's remedy is to strengthen the
 * scrims, never to change the type, so the glyph colours below are constants
 * the test reads the background against.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class NowPlayingLegibilityTest {
    @Test
    fun a_near_white_cover_leaves_the_title_and_the_artist_line_readable() {
        val background = titleBandBackground(nearWhiteArtwork())

        val title = contrast(relativeLuminance(1f, 1f, 1f), relativeLuminance(background))
        val artist = contrast(
            relativeLuminance(compositeWhite(background, ARTIST_ALPHA)),
            relativeLuminance(background),
        )

        assertTrue(
            "title contrast on a near-white cover is $title:1, below the 4.5:1 this scene owes it",
            title >= 4.5f,
        )
        assertTrue(
            "artist contrast on a near-white cover is $artist:1, below the 3:1 floor",
            artist >= 3.0f,
        )
    }

    /**
     * Mean channel value, 0..1, of the strip the title and artist rows occupy.
     *
     * The scene places the title block at the cover centre plus 156 dp, so the
     * strip is measured from there. Density 1 keeps the raster small; every
     * offset in the scene is expressed in dp, so the geometry is unchanged.
     */
    private fun titleBandBackground(artwork: Bitmap): Float {
        val width = 448
        val height = 880
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        val fog = prepareCoverFogBitmap(artwork, Color.WHITE)
        CanvasDrawScope().draw(
            density = Density(1f),
            layoutDirection = LayoutDirection.Ltr,
            canvas = Canvas(bitmap.asImageBitmap()),
            size = Size(width.toFloat(), height.toFloat()),
        ) {
            drawRect(androidx.compose.ui.graphics.Color.Black)
            drawNowPlayingFog(
                fog = fog,
                center = Offset(size.width / 2f, size.height * PLAYED_CENTRE_FRACTION),
                angleA = 0f,
                angleB = 0f,
                fogLevel = 1f,
                bassPressure = 1f,
                opacity = 1f,
                rotationsEnabled = false,
            )
        }

        val pixels = IntArray(width * height)
        bitmap.getPixels(pixels, 0, width, 0, 0, width, height)

        val centreY = (height * PLAYED_CENTRE_FRACTION).toInt()
        val top = centreY + TITLE_BLOCK_TOP_DP
        val bottom = centreY + TITLE_BLOCK_BOTTOM_DP
        var total = 0L
        var counted = 0
        for (y in top until bottom) {
            for (x in TITLE_INSET_DP until width - TITLE_INSET_DP) {
                val pixel = pixels[y * width + x]
                total += Color.red(pixel) + Color.green(pixel) + Color.blue(pixel)
                counted += 3
            }
        }
        return total.toFloat() / counted / 255f
    }

    private fun nearWhiteArtwork(): Bitmap = solidArtwork(230)

    private fun solidArtwork(value: Int): Bitmap =
        Bitmap.createBitmap(8, 8, Bitmap.Config.ARGB_8888).apply {
            for (y in 0 until height) {
                for (x in 0 until width) {
                    setPixel(x, y, Color.rgb(value, value, value))
                }
            }
        }

    /** White laid over the measured background at the artist row's own alpha. */
    private fun compositeWhite(background: Float, alpha: Float): Float =
        alpha + (1f - alpha) * background

    private fun relativeLuminance(value: Float): Float =
        relativeLuminance(value, value, value)

    private fun relativeLuminance(red: Float, green: Float, blue: Float): Float =
        0.2126f * channel(red) + 0.7152f * channel(green) + 0.0722f * channel(blue)

    private fun channel(value: Float): Float = if (value <= 0.03928f) {
        value / 12.92f
    } else {
        Math.pow(((value + 0.055f) / 1.055f).toDouble(), 2.4).toFloat()
    }

    private fun contrast(one: Float, other: Float): Float {
        val lighter = maxOf(one, other)
        val darker = minOf(one, other)
        return (lighter + 0.05f) / (darker + 0.05f)
    }

    private companion object {
        /** Mirrors `NowPlayingScene`'s played-state cover placement. */
        const val PLAYED_CENTRE_FRACTION = 0.34f

        /** The title block starts 156 dp under the cover centre and runs ~90 dp. */
        const val TITLE_BLOCK_TOP_DP = 156
        const val TITLE_BLOCK_BOTTOM_DP = 246
        const val TITLE_INSET_DP = 28

        /** `SceneTitle` draws the artist line at this alpha over white. */
        const val ARTIST_ALPHA = 0.62f
    }
}
