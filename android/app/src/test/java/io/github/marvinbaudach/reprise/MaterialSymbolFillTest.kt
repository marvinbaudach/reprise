package io.github.marvinbaudach.reprise

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
 * The production heart has to *look* different when it is a favourite.
 *
 * Every test we had about ratings asserted on semantics or on a boolean this
 * code computes itself, and all of them stayed green while the sheet drew five
 * identical outlines at every rating: Material Symbols keeps "filled" on the
 * variable-font axis `FILL`, so changing ligature names is not enough to make
 * the two states render different pixels.
 * A test that can catch that has to look at the pixels, so this one draws both
 * states through the real rasteriser and counts the ink.
 *
 * One composition is rendered twice with only the state moved between the two
 * shots, which keeps the heart in the same place and makes the two bitmaps
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
    fun theFilledHeartDrawsMoreInkThanTheOutlinedOne() {
        showTheHeart()

        val outlinedInk = renderHeart(filled = false).litPixels()
        val filledInk = renderHeart(filled = true).litPixels()

        assertTrue(
            "the outlined heart drew no ink at all ($outlinedInk lit pixels), " +
                "so this test cannot tell the two states apart",
            outlinedInk > 0,
        )
        assertTrue(
            "a filled heart must cover markedly more of its box than an outlined " +
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
    fun theFavouriteAndOrdinaryHeartAreNotTheSameBitmap() {
        showTheHeart()

        val outlined = renderHeart(filled = false)
        val outlinedInk = outlined.litPixels()
        val filledShot = renderHeart(filled = true)

        val differing = (0 until outlined.height).sumOf { y ->
            (0 until outlined.width).count { x -> outlined[x, y] != filledShot[x, y] }
        }
        val needed = outlinedInk / CHANGED_INK_DIVISOR
        assertTrue(
            "moving the favourite state must repaint a real part of the heart, but only " +
                "$differing pixels changed against an outlined heart of " +
                "$outlinedInk pixels (needed $needed); zero changed pixels is " +
                "what would ship a favourite nobody could see",
            differing >= needed && differing > 0,
        )
    }

    /**
     * A white glyph on black, filling the window so the count is about the
     * glyph and not about anti-aliasing at a badge's 14sp.
     */
    private fun showTheHeart() {
        compose.setContent { HeartUnderTest() }
        compose.waitForIdle()
    }

    @Composable
    private fun HeartUnderTest() {
        Box(
            modifier = Modifier.fillMaxSize().background(Color.Black),
            contentAlignment = Alignment.Center,
        ) {
            FavouriteHeartIcon(
                favourite = filled,
                contentDescription = "",
                tint = Color.White,
                sizeSp = HEART_SIZE_SP,
            )
        }
    }

    /** Moves the state, lets Compose settle, and rasterises what it drew. */
    private fun renderHeart(filled: Boolean): androidx.compose.ui.graphics.PixelMap {
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
        const val HEART_SIZE_SP = 48
        const val LIT_THRESHOLD = 0.5f

        /**
         * At `FILL = 1` the heart is solid where the outline is a ring, which
         * the font's own outlines put at 1.37x the ink (165078 -> 226791 units²
         * at wght 400, opsz 24) and the rasteriser reproduces at 1.34x. The bar
         * sits between that and the 1.00x two identical glyphs would give, far
         * enough from both to be neither a rasteriser fingerprint nor a pass
         * for the bug.
         */
        const val MINIMUM_INK_RATIO = 1.2f

        /** A fifth of the outlined heart's ink has to change; the bug changed none. */
        const val CHANGED_INK_DIVISOR = 5
    }
}
