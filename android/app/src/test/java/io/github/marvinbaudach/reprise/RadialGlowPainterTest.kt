package io.github.marvinbaudach.reprise

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.RadialGradient
import android.graphics.Shader
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class RadialGlowPainterTest {
    @Test
    fun movingAndResizingTheGlowKeepsTheShaderAndItsNativeInstance() {
        val painter = RadialGlowPainter()
        val canvas = Canvas(Bitmap.createBitmap(SIDE, SIDE, Bitmap.Config.ARGB_8888))
        val blue = Color(0f, 0f, 1f, 0.8f)

        painter.draw(canvas, Offset(20f, 30f), 14f, blue)
        val first = painter.shaderFor(0x0000ff)
        painter.draw(canvas, Offset(70f, 40f), 25f, blue.copy(alpha = 0.3f))

        assertSame(first, painter.shaderFor(0x0000ff))
        // Shader.setLocalMatrix discards the native shader, and the next Paint.getNativeInstance
        // rebuilds it: simpleperf on a Pixel put RadialGradient_create at 2.8 % of the main thread
        // (about 4 % with the Skia gradient setup) while the spectrum visualizer played. A shader
        // that still reports an identity local matrix has never been invalidated by a draw.
        assertFalse(first.getLocalMatrix(Matrix()))
    }

    @Test
    fun theGlowRendersTheSamePixelsAsAShaderPlacedByALocalMatrix() {
        val centre = Offset(41f, 37f)
        val radius = 22f
        val colour = Color(0.9f, 0.2f, 0.4f, 0.6f)
        val actual = Bitmap.createBitmap(SIDE, SIDE, Bitmap.Config.ARGB_8888)
        RadialGlowPainter().draw(Canvas(actual), centre, radius, colour)

        val expected = Bitmap.createBitmap(SIDE, SIDE, Bitmap.Config.ARGB_8888)
        val rgb = 0xe63366
        val shader = RadialGradient(
            0f, 0f, 1f, rgb or OPAQUE, rgb, Shader.TileMode.CLAMP,
        ).apply {
            setLocalMatrix(
                Matrix().apply {
                    setScale(radius, radius)
                    postTranslate(centre.x, centre.y)
                },
            )
        }
        val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            this.shader = shader
            alpha = (0.6f * 255f).toInt()
        }
        Canvas(expected).drawCircle(centre.x, centre.y, radius, paint)

        assertArrayEquals(expected.pixels(), actual.pixels())
    }

    private fun Bitmap.pixels(): IntArray =
        IntArray(width * height).also { getPixels(it, 0, width, 0, 0, width, height) }

    private companion object {
        const val SIDE = 100
        const val OPAQUE = -0x1000000
    }
}
