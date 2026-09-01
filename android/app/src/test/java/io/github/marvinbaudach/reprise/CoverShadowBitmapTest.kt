package io.github.marvinbaudach.reprise

import android.graphics.Bitmap
import android.graphics.Color
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Canvas
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.CanvasDrawScope
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class CoverShadowBitmapTest {
    @Test
    fun an_invisible_panel_leaves_no_cover_shadow() {
        val rendered = Bitmap.createBitmap(CANVAS_SIZE, CANVAS_SIZE, Bitmap.Config.ARGB_8888)
        CanvasDrawScope().draw(
            density = Density(1f),
            layoutDirection = LayoutDirection.Ltr,
            canvas = Canvas(rendered.asImageBitmap()),
            size = Size(CANVAS_SIZE.toFloat(), CANVAS_SIZE.toFloat()),
        ) {
            drawPlayedCover(
                artwork = null,
                center = Offset(COVER_CENTRE, COVER_CENTRE),
                fallback = androidx.compose.ui.graphics.Color.White,
                shadow = prepareCoverShadowBitmap(),
                opacity = 0f,
            )
        }

        val nonTransparent = IntArray(CANVAS_SIZE * CANVAS_SIZE).also { pixels ->
            rendered.getPixels(pixels, 0, CANVAS_SIZE, 0, 0, CANVAS_SIZE, CANVAS_SIZE)
        }.count { pixel -> Color.alpha(pixel) > 0 }
        assertEquals(0, nonTransparent)
    }

    @Test
    fun shadow_below_cover_falls_monotonically_without_stair_steps() {
        val rendered = Bitmap.createBitmap(CANVAS_SIZE, CANVAS_SIZE, Bitmap.Config.ARGB_8888)
        val artwork = Bitmap.createBitmap(8, 8, Bitmap.Config.ARGB_8888).apply {
            eraseColor(Color.WHITE)
        }
        CanvasDrawScope().draw(
            density = Density(1f),
            layoutDirection = LayoutDirection.Ltr,
            canvas = Canvas(rendered.asImageBitmap()),
            size = Size(CANVAS_SIZE.toFloat(), CANVAS_SIZE.toFloat()),
        ) {
            drawPlayedCover(
                artwork = artwork.asImageBitmap(),
                center = Offset(COVER_CENTRE, COVER_CENTRE),
                fallback = androidx.compose.ui.graphics.Color.Black,
                shadow = prepareCoverShadowBitmap(),
            )
        }

        val alphas = (COVER_BOTTOM + 1..COVER_BOTTOM + SAMPLE_DEPTH).map { y ->
            Color.alpha(rendered.getPixel(COVER_CENTRE.toInt(), y))
        }

        assertTrue("the shadow must start visibly below the cover: $alphas", alphas.first() > 0)
        // One hump, not a staircase. The shadow is cast downwards, so its
        // darkest point sits below the cover's edge rather than at it — what
        // must never happen is alpha going back up once it has started to
        // fall, which is exactly what stacked rectangles did.
        val peak = alphas.indexOf(alphas.max())
        alphas.take(peak + 1).zipWithNext().forEach { (upper, lower) ->
            assertTrue("shadow alpha must rise smoothly into its peak: $alphas", lower >= upper)
        }
        alphas.drop(peak).zipWithNext().forEach { (upper, lower) ->
            assertTrue("shadow alpha must fall monotonically: $upper then $lower", lower <= upper)
        }
        // The shadow has to run out, not stop: a single count of alpha 96 px
        // below the cover is one 255th and invisible, but a hard 0 here would
        // only be measuring where the sample happens to end.
        assertTrue("the shadow must fade out, not end abruptly: $alphas", alphas.last() <= 2)
        assertTrue(
            "a soft shadow needs a gradient, not a handful of stacked alpha steps: $alphas",
            alphas.distinct().size >= MIN_GRADIENT_LEVELS,
        )
    }

    private companion object {
        const val CANVAS_SIZE = 512
        const val COVER_CENTRE = 256f
        const val COVER_BOTTOM = 392
        const val SAMPLE_DEPTH = 96
        const val MIN_GRADIENT_LEVELS = 16
    }
}
