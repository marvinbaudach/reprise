package de.reprise.spike

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
            flatRect(x = 12f, y = 16f, width = 14f, height = 64f),
        )
        val resting = render(
            flatRect(x = 42f, y = 68f, width = 14f, height = 12f),
        )

        assertTrue(signal.litPixels() > 700)
        assertTrue(resting.litPixels() > 100)
    }

    @Test
    fun noSceneBufferDrawsNothing() {
        assertEquals(0, render(emptyList()).litPixels())
    }

    @Test
    fun everyFlatGeometryKindUsesTheDocumentedRecordLayout() {
        val scene = buildList {
            addAll(flatRect(x = 5f, y = 5f, width = 15f, height = 15f))
            addAll(
                listOf(
                    1f, 0f, 1f, 0f, 1f, 2f, 0.6f, 3f,
                    30f, 10f, 45f, 25f, 60f, 10f,
                ),
            )
            addAll(
                listOf(
                    2f, 0f, 0f, 1f, 0.8f, 0f, 0f, 3f,
                    80f, 25f, 14f,
                ),
            )
        }

        val bitmap = render(scene)

        assertTrue(Color.red(bitmap.getPixel(10, 10)) > 150)
        assertTrue(Color.green(bitmap.getPixel(45, 24)) > 80)
        assertTrue(Color.blue(bitmap.getPixel(80, 25)) > 80)
    }

    private fun render(scene: List<Float>): Bitmap {
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

    private fun flatRect(x: Float, y: Float, width: Float, height: Float): List<Float> = listOf(
        0f, 1f, 0.2f, 0.8f, 1f, 0f, 0f, 4f,
        x, y, width, height,
    )

    private companion object {
        const val SIDE = 100
    }
}
