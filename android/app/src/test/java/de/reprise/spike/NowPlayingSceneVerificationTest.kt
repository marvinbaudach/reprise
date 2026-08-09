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
import de.reprise.spike.scene.CoreShape
import de.reprise.spike.scene.SceneState
import de.reprise.spike.scene.SpectrogramFrames
import org.junit.Assert.assertArrayEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class NowPlayingSceneVerificationTest {
    @Test
    fun thirty_second_angle_trace_is_identical_across_two_replays() {
        val frames = patternedFrames(frameCount = 30 * 20)

        val first = angleTrace(frames)
        val second = angleTrace(frames)

        assertArrayEquals(first, second)
    }

    @Test
    fun paused_fog_and_corona_raster_is_pixel_identical_three_seconds_later() {
        val state = SceneState(patternedFrames(frameCount = 80), CoreShape("Still", "Reprise"))
        state.advanceTo(40)
        val fog = prepareCoverFogBitmap(greyscaleArtwork(), Color.DKGRAY)
        val before = renderScene(state, fog)

        repeat(3 * 20) { state.advanceTo(40) }

        assertArrayEquals(before, renderScene(state, fog))
    }

    private fun angleTrace(frames: SpectrogramFrames): IntArray {
        val state = SceneState(frames, CoreShape("Repeatable", "Reprise"))
        return IntArray(frames.frameCount * 2).also { trace ->
            repeat(frames.frameCount) { frame ->
                state.advanceTo(frame)
                trace[frame * 2] = state.fogAngleA.toRawBits()
                trace[frame * 2 + 1] = state.fogAngleB.toRawBits()
            }
        }
    }

    private fun renderScene(state: SceneState, fog: CoverFogBitmap): IntArray {
        val bitmap = Bitmap.createBitmap(240, 400, Bitmap.Config.ARGB_8888)
        CanvasDrawScope().draw(
            density = Density(1f),
            layoutDirection = LayoutDirection.Ltr,
            canvas = Canvas(bitmap.asImageBitmap()),
            size = Size(240f, 400f),
        ) {
            drawRect(androidx.compose.ui.graphics.Color.Black)
            drawNowPlayingFog(
                fog = fog,
                center = Offset(size.width / 2f, size.height * 0.34f),
                angleA = state.fogAngleA,
                angleB = state.fogAngleB,
                opacity = 0.5f,
                rotationsEnabled = true,
            )
            drawNowPlayingBurst(
                state = state,
                bloomBuffer = BurstBloomBuffer(),
                opacity = 0.5f,
                effects = BurstEffects(bloom = false, hotRay = false),
            )
        }
        return IntArray(bitmap.width * bitmap.height).also { pixels ->
            bitmap.getPixels(pixels, 0, bitmap.width, 0, 0, bitmap.width, bitmap.height)
        }
    }

    private fun patternedFrames(frameCount: Int): SpectrogramFrames = SpectrogramFrames(
        bandCount = 24,
        frameRateHz = 20,
        cells = ByteArray(frameCount * 24) { index ->
            val frame = index / 24
            val band = index % 24
            ((frame * 19 + band * 11) % 256).toByte()
        },
    )

    private fun greyscaleArtwork(): Bitmap = Bitmap.createBitmap(
        8,
        8,
        Bitmap.Config.ARGB_8888,
    ).apply {
        for (y in 0 until height) {
            for (x in 0 until width) {
                val value = (x + y) * 16
                setPixel(x, y, Color.rgb(value, value, value))
            }
        }
    }
}
