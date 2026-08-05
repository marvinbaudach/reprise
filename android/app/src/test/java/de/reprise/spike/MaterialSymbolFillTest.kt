package de.reprise.spike

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.ViewGroup
import androidx.activity.ComponentActivity
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * The star has to *look* different when it is rated.
 *
 * Every test we had about ratings asserted on semantics or on a boolean this
 * code computes itself, and all of them stayed green while the sheet drew five
 * identical outlines at every rating: Material Symbols keeps "filled" on the
 * variable-font axis `FILL`, so the `star_outline` ligature resolves to the
 * very same glyph id as `star` and the two states rendered the same pixels.
 * A test that can catch that has to look at the pixels, so this one draws both
 * states through the real rasteriser and counts the ink.
 *
 * One composition is rendered twice with only the state moved between the two
 * shots, which keeps the star in the same place and makes the two bitmaps
 * comparable pixel for pixel. `captureToImage()` is not used: its `forceRedraw`
 * waits for a frame callback Robolectric's looper never delivers, so it times
 * out. Drawing the laid-out view into a software bitmap goes through the same
 * Skia rasteriser and the same typeface, which is the part under test.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h1000dp")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class MaterialSymbolFillTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private var filled by mutableStateOf(false)

    @Test
    fun theFilledStarDrawsMoreInkThanTheOutlinedOne() {
        showTheStar()

        val outlinedInk = renderStar(filled = false).litPixels()
        val filledInk = renderStar(filled = true).litPixels()

        assertTrue(
            "the outlined star drew no ink at all ($outlinedInk lit pixels), " +
                "so this test cannot tell the two states apart",
            outlinedInk > 0,
        )
        assertTrue(
            "a filled star must cover markedly more of its box than an outlined " +
                "one, but filled drew $filledInk lit pixels against outlined's " +
                "$outlinedInk (ratio ${filledInk.toFloat() / outlinedInk}, " +
                "needed $MINIMUM_INK_RATIO)",
            filledInk >= outlinedInk * MINIMUM_INK_RATIO,
        )
    }

    /**
     * The same guard from the other side: if the two states ever collapse onto
     * one glyph again the bitmaps become identical, and identical is the one
     * outcome that must never be reached however the ink happens to be counted.
     */
    @Test
    fun theRatedAndUnratedStarAreNotTheSameBitmap() {
        showTheStar()

        val outlined = renderStar(filled = false)
        val outlinedInk = outlined.litPixels()
        val filledShot = renderStar(filled = true)

        val differing = (0 until outlined.height).sumOf { y ->
            (0 until outlined.width).count { x -> outlined[x, y] != filledShot[x, y] }
        }
        val needed = outlinedInk / CHANGED_INK_DIVISOR
        assertTrue(
            "moving the rating must repaint a real part of the star, but only " +
                "$differing pixels changed against an outlined star of " +
                "$outlinedInk pixels (needed $needed); zero changed pixels is " +
                "what shipped a rating nobody could see",
            differing >= needed && differing > 0,
        )
    }

    /**
     * A white glyph on black, filling the window so the count is about the
     * glyph and not about anti-aliasing at a badge's 14sp.
     */
    private fun showTheStar() {
        compose.setContent { StarUnderTest() }
        compose.waitForIdle()
    }

    @Composable
    private fun StarUnderTest() {
        Box(
            modifier = Modifier.fillMaxSize().background(Color.Black),
            contentAlignment = Alignment.Center,
        ) {
            MaterialSymbol(
                name = "star",
                contentDescription = "",
                tint = Color.White,
                sizeSp = STAR_SIZE_SP,
                filled = filled,
            )
        }
    }

    /** Moves the state, lets Compose settle, and rasterises what it drew. */
    private fun renderStar(filled: Boolean): androidx.compose.ui.graphics.PixelMap {
        this.filled = filled
        compose.waitForIdle()
        val content = compose.activity.findViewById<ViewGroup>(android.R.id.content)
        val bitmap = Bitmap.createBitmap(content.width, content.height, Bitmap.Config.ARGB_8888)
        content.draw(Canvas(bitmap))
        return bitmap.asImageBitmap().toPixelMap()
    }

    /** Pixels the glyph actually painted: white ink on the black background. */
    private fun androidx.compose.ui.graphics.PixelMap.litPixels(): Int =
        (0 until height).sumOf { y ->
            (0 until width).count { x ->
                val pixel = this[x, y]
                (pixel.red + pixel.green + pixel.blue) / 3f > LIT_THRESHOLD
            }
        }

    private companion object {
        const val STAR_SIZE_SP = 48
        const val LIT_THRESHOLD = 0.5f

        /**
         * At `FILL = 1` the star is solid where the outline is a ring, which
         * the font's own outlines put at 1.37x the ink (165078 -> 226791 units²
         * at wght 400, opsz 24) and the rasteriser reproduces at 1.34x. The bar
         * sits between that and the 1.00x two identical glyphs would give, far
         * enough from both to be neither a rasteriser fingerprint nor a pass
         * for the bug.
         */
        const val MINIMUM_INK_RATIO = 1.2f

        /** A fifth of the outlined star's ink has to change; the bug changed none. */
        const val CHANGED_INK_DIVISOR = 5
    }
}
