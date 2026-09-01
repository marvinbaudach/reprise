package io.github.marvinbaudach.reprise

import android.graphics.Bitmap
import android.graphics.Color
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Canvas
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.CanvasDrawScope
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import java.nio.ByteBuffer
import java.nio.ByteOrder
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
class VisualizerScenePixelsTest {
    @Test
    fun barsAreVisibleUnderSignalAndTheSharedRestingScene() {
        val signal = render(
            flatScene(flatRect(x = 12f, y = 16f, width = 14f, height = 64f)),
        )
        val resting = render(
            flatScene(flatRect(x = 42f, y = 68f, width = 14f, height = 12f)),
        )

        assertTrue(signal.litPixels() > 700)
        assertTrue(resting.litPixels() > 100)
    }

    @Test
    fun noSceneBufferDrawsNothing() {
        assertEquals(0, render(byteArrayOf()).litPixels())
    }

    @Test
    fun everyFlatGeometryKindUsesTheDocumentedRecordLayout() {
        val scene = flatScene(
            flatRect(x = 5f, y = 5f, width = 15f, height = 15f),
            floatArrayOf(
                1f, 0f, 1f, 0f, 1f, 2f, 0.6f, 3f,
                30f, 10f, 45f, 25f, 60f, 10f,
            ),
            floatArrayOf(
                2f, 0f, 0f, 1f, 0.8f, 0f, 0f, 3f,
                80f, 25f, 14f,
            ),
        )

        val bitmap = render(scene)

        assertTrue(Color.red(bitmap.getPixel(10, 10)) > 150)
        assertTrue(Color.green(bitmap.getPixel(45, 24)) > 80)
        assertTrue(Color.blue(bitmap.getPixel(80, 25)) > 80)
    }

    @Test
    fun aTruncatedBufferFailsClosed() {
        val complete = flatScene(flatRect(x = 12f, y = 16f, width = 14f, height = 64f))
        val hostilePointCount = flatScene(
            floatArrayOf(
                1f, 1f, 1f, 1f, 1f, 2f, 0f, 1_000_001f,
            ),
        )

        assertEquals(0, render(complete.copyOf(complete.size - 1)).litPixels())
        assertEquals(0, render(hostilePointCount).litPixels())
    }

    private fun render(scene: ByteArray): Bitmap {
        val bitmap = Bitmap.createBitmap(SIDE, SIDE, Bitmap.Config.ARGB_8888)
        CanvasDrawScope().draw(
            density = Density(1f),
            layoutDirection = LayoutDirection.Ltr,
            canvas = Canvas(bitmap.asImageBitmap()),
            size = Size(SIDE.toFloat(), SIDE.toFloat()),
        ) {
            drawRect(androidx.compose.ui.graphics.Color.Black)
            drawVisualizerScene(
                buffer = scene,
                bounds = Rect(Offset.Zero, size),
            )
        }
        return bitmap
    }

    private fun Bitmap.litPixels(): Int {
        val pixels = IntArray(width * height)
        getPixels(pixels, 0, width, 0, 0, width, height)
        return pixels.count { Color.red(it) + Color.green(it) + Color.blue(it) > 30 }
    }

    private fun flatRect(x: Float, y: Float, width: Float, height: Float): FloatArray = floatArrayOf(
        0f, 1f, 0.2f, 0.8f, 1f, 0f, 0f, 4f,
        x, y, width, height,
    )

    private fun flatScene(vararg records: FloatArray): ByteArray {
        val values = records.sumOf(FloatArray::size)
        return ByteBuffer.allocate(values * Float.SIZE_BYTES)
            .order(ByteOrder.LITTLE_ENDIAN)
            .apply {
                records.forEach { record -> record.forEach(::putFloat) }
            }
            .array()
    }

    private companion object {
        const val SIDE = 100
    }
}
