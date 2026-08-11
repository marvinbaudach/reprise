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
import de.reprise.spike.scene.SceneState
import de.reprise.spike.scene.SpectrogramFrames
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import kotlin.math.roundToInt

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class NowPlayingSceneVerificationTest {
    @Test
    fun played_fog_render_answers_the_settled_slow_envelope_without_drifting() {
        val quietState = settledState(level = 0.126f)
        val loudState = settledState(level = 0.646f)
        val fog = prepareCoverFogBitmap(solidArtwork(Color.WHITE), Color.DKGRAY)

        val quiet = renderPlayedFog(quietState, fog)
        val loud = renderPlayedFog(loudState, fog)
        val quietLuma = meanFogRegionLuma(quiet)
        val loudLuma = meanFogRegionLuma(loud)

        assertTrue(
            "settled loud fog luma $loudLuma must exceed quiet $quietLuma by at least 25%",
            loudLuma >= quietLuma * 1.25f,
        )
        assertArrayEquals(loud, renderPlayedFog(loudState, fog))
    }

    @Test
    fun thirty_second_angle_trace_is_identical_across_two_replays() {
        val frames = patternedFrames(frameCount = 30 * 20)

        val first = angleTrace(frames)
        val second = angleTrace(frames)

        assertArrayEquals(first, second)
    }

    @Test
    fun paused_non_empty_fog_keeps_turning_and_changes_the_rendered_raster() {
        val frames = patternedFrames(frameCount = 80)
        val state = SceneState(frames)
        val clock = AdvancingSceneClock()
        val paused = ScenePositionSample(
            positionMs = PAUSED_POSITION_MS,
            observedAtNanos = 0,
            playing = false,
        )
        val driver = SceneDriver(frames, state, clock, ScenePositionSource { paused }) { true }
        driver.tick()
        val fog = prepareCoverFogBitmap(greyscaleArtwork(), Color.DKGRAY)
        val angleBefore = state.fogAngleA
        val before = renderScene(state, fog)

        repeat(PAUSED_FRAME_COUNT) {
            clock.nanos += FRAME_INTERVAL_NANOS
            driver.tick()
        }

        assertEquals(PAUSED_FRAME_INDEX, driver.lastDrivenFrameIndex)
        assertTrue(state.fogAngleA > angleBefore)
        assertFalse(before.contentEquals(renderScene(state, fog)))
    }

    private fun angleTrace(frames: SpectrogramFrames): IntArray {
        val state = SceneState(frames)
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
                fogLevel = state.fogLevel,
                bassPressure = state.bassPressure,
                opacity = 0.5f,
                rotationsEnabled = true,
            )
        }
        return IntArray(bitmap.width * bitmap.height).also { pixels ->
            bitmap.getPixels(pixels, 0, bitmap.width, 0, 0, bitmap.width, bitmap.height)
        }
    }

    private fun renderPlayedFog(state: SceneState, fog: CoverFogBitmap): IntArray {
        val bitmap = Bitmap.createBitmap(RENDER_WIDTH, RENDER_HEIGHT, Bitmap.Config.ARGB_8888)
        CanvasDrawScope().draw(
            density = Density(1f),
            layoutDirection = LayoutDirection.Ltr,
            canvas = Canvas(bitmap.asImageBitmap()),
            size = Size(RENDER_WIDTH.toFloat(), RENDER_HEIGHT.toFloat()),
        ) {
            drawRect(androidx.compose.ui.graphics.Color.Black)
            drawPlayedNowPlayingFog(
                fog = fog,
                center = Offset(size.width / 2f, size.height * PLAYED_CENTRE_FRACTION),
                state = state,
                opacity = 1f,
                rotationsEnabled = false,
            )
        }
        return IntArray(bitmap.width * bitmap.height).also { pixels ->
            bitmap.getPixels(pixels, 0, bitmap.width, 0, 0, bitmap.width, bitmap.height)
        }
    }

    /**
     * Mean sRGB luma above the title scrim's first stop.
     *
     * At this raster size the scrim starts at y=264 (cover centre 136 plus
     * 128 dp), so the sampled y=104..239 band cannot hide the fog response.
     */
    private fun meanFogRegionLuma(pixels: IntArray): Float {
        var total = 0.0
        var count = 0
        for (y in FOG_SAMPLE_TOP until FOG_SAMPLE_BOTTOM) {
            for (x in FOG_SAMPLE_INSET until RENDER_WIDTH - FOG_SAMPLE_INSET) {
                val pixel = pixels[y * RENDER_WIDTH + x]
                total += 0.2126 * Color.red(pixel) +
                    0.7152 * Color.green(pixel) +
                    0.0722 * Color.blue(pixel)
                count += 1
            }
        }
        return (total / count / 255.0).toFloat()
    }

    private fun settledState(level: Float): SceneState {
        val cell = (level.coerceIn(0f, 1f) * 255f).roundToInt()
        val frames = SpectrogramFrames(
            bandCount = 24,
            frameRateHz = 20,
            cells = ByteArray(SETTLE_FRAME_COUNT * 24) { cell.toByte() },
        )
        return SceneState(frames).also { state ->
            repeat(frames.frameCount) { frame -> state.advanceTo(frame) }
        }
    }

    private fun solidArtwork(colour: Int): Bitmap = Bitmap.createBitmap(
        8,
        8,
        Bitmap.Config.ARGB_8888,
    ).apply { eraseColor(colour) }

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

    /** A clock the test moves by hand while the paused media position stays fixed. */
    private class AdvancingSceneClock(var nanos: Long = 0) : SceneClock {
        override fun nowNanos(): Long = nanos
    }
}

/** Far enough into the track that any extrapolation leaves the paused frame. */
private const val PAUSED_POSITION_MS = 2_000L

/** Where [PAUSED_POSITION_MS] falls at the fixture's 20 Hz frame rate. */
private const val PAUSED_FRAME_INDEX = 40

/** Three seconds of the 60 Hz frame loop the scene is actually driven by. */
private const val PAUSED_FRAME_COUNT = 3 * 60
private const val FRAME_INTERVAL_NANOS = 16_666_667L
private const val RENDER_WIDTH = 240
private const val RENDER_HEIGHT = 400
private const val PLAYED_CENTRE_FRACTION = 0.34f
private const val FOG_SAMPLE_TOP = 104
private const val FOG_SAMPLE_BOTTOM = 240
private const val FOG_SAMPLE_INSET = 24

/** Four seconds is twenty attack constants for the 200 ms slow fog follower. */
private const val SETTLE_FRAME_COUNT = 4 * 20
