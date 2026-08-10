package de.reprise.spike

import android.graphics.Bitmap
import android.graphics.Color
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Canvas
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.CanvasDrawScope
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import de.reprise.spike.scene.CoreShape
import de.reprise.spike.scene.SceneState
import de.reprise.spike.scene.SpectrogramFrames
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class NowPlayingBurstPixelsTest {
    @Test
    fun wedges_light_every_corner_without_needing_bloom_or_a_hot_ray() {
        val bitmap = render(sceneState("Track", "Artist", cell = 180), BurstBloomBuffer())

        listOf(
            4 to 4,
            bitmap.width - 5 to 4,
            4 to bitmap.height - 5,
            bitmap.width - 5 to bitmap.height - 5,
        ).forEach { (x, y) ->
            val pixel = bitmap.getPixel(x, y)
            assertTrue(
                "corner ($x, $y) must carry wedge colour, got ${Integer.toHexString(pixel)}",
                Color.red(pixel) + Color.green(pixel) + Color.blue(pixel) > 12,
            )
        }
    }

    @Test
    fun a_reused_buffer_redraws_a_state_identically_after_another_track() {
        val buffer = BurstBloomBuffer()
        val first = sceneState("Track", "Artist", cell = 180)
        val second = sceneState("Other", "Band", cell = 60)

        val before = render(first, buffer).pixels()
        val between = render(second, buffer).pixels()
        val after = render(first, buffer).pixels()

        assertFalse(
            "the two tracks must render differently, or nothing could go stale",
            before.contentEquals(between),
        )
        assertArrayEquals(before, after)
    }

    private fun sceneState(title: String, artist: String, cell: Int): SceneState = SceneState(
        SpectrogramFrames(24, 20, ByteArray(24) { cell.toByte() }),
        CoreShape(title, artist),
    ).apply { advanceTo(0) }

    private fun render(state: SceneState, buffer: BurstBloomBuffer): Bitmap {
        val bitmap = Bitmap.createBitmap(240, 400, Bitmap.Config.ARGB_8888)
        CanvasDrawScope().draw(
            density = Density(1f),
            layoutDirection = LayoutDirection.Ltr,
            canvas = Canvas(bitmap.asImageBitmap()),
            size = Size(240f, 400f),
        ) {
            drawNowPlayingBurst(
                state = state,
                bloomBuffer = buffer,
                opacity = 1f,
                effects = BurstEffects(bloom = false, hotRay = false),
            )
        }
        return bitmap
    }

    private fun Bitmap.pixels(): IntArray = IntArray(width * height).also { pixels ->
        getPixels(pixels, 0, width, 0, 0, width, height)
    }
}
