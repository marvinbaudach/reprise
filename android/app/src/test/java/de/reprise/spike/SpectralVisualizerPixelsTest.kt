package de.reprise.spike

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.ViewGroup
import androidx.activity.ComponentActivity
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.shape.RoundedCornerShape
import de.reprise.spike.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.reprise_android_ffi.AndroidColorScheme

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w240dp-h320dp")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class SpectralVisualizerPixelsTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val renderData = VisualizerRenderData()

    @Test
    fun spectrumDrawsTheCurrentColumnWithTheRustBandColours() {
        renderData.column = List(BAND_COUNT) { 255 }
        showVisualizer(MobileVisualizer.SPECTRUM)

        val pixels = render()
        val low = bandColours.first()
        val high = bandColours.last()
        assertTrue(
            "the low band must reach the canvas in Rust's coral, but no matching pixels were drawn",
            pixels.pixelsNear(low.red, low.green, low.blue) >=
                MINIMUM_BAND_PIXELS,
        )
        assertTrue(
            "the high band must reach the canvas in Rust's teal, but no matching pixels were drawn",
            pixels.pixelsNear(high.red, high.green, high.blue) >=
                MINIMUM_BAND_PIXELS,
        )
        assertEquals(PLAYBACK_FRACTION, renderData.lastPosition, FRACTION_TOLERANCE)
    }

    @Test
    fun previewDrawsTheSeekBarsWithoutAPlayheadOrTimeLabels() {
        renderData.barAnswer = { count ->
            List(count) {
                TrackRenderBar(
                    silence = false,
                    level = 1f,
                    red = PREVIEW_RED,
                    green = PREVIEW_GREEN,
                    blue = PREVIEW_BLUE,
                )
            }
        }
        showVisualizer(MobileVisualizer.PREVIEW_BAND)

        val previewPixels = render().pixelsNear(
            red = PREVIEW_RED,
            green = PREVIEW_GREEN,
            blue = PREVIEW_BLUE,
            firstRow = VISUALIZER_SIZE_DP,
        )
        assertTrue(
            "Preview must draw the seek bar's own RGB cells, but only $previewPixels pixels did",
            previewPixels >= MINIMUM_PREVIEW_PIXELS,
        )
        compose.onNodeWithTag("now-playing-seek").assertDoesNotExist()
        assertTrue("Preview must ask for the bar count that fits its width", renderData.lastBarCount > 1)
    }

    private fun showVisualizer(mode: MobileVisualizer) {
        val theme = MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.SYSTEM,
            dynamicAvailable = false,
        )
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                CompositionLocalProvider(
                    LocalTrackRenderData provides renderData,
                    LocalVisualizerControl provides VisualizerControl(mode) {},
                    LocalSpectralBandColours provides bandColours,
                ) {
                    Box(Modifier.fillMaxSize().background(Color.Black)) {
                        NowPlayingVisualizer(
                            trackId = TRACK_ID,
                            trackUri = "content://provider/document/track.flac",
                            playbackFraction = PLAYBACK_FRACTION,
                            size = VISUALIZER_SIZE_DP,
                            shape = RoundedCornerShape(12.dp),
                        )
                    }
                }
            }
        }
        compose.waitForIdle()
    }

    private fun render(): androidx.compose.ui.graphics.PixelMap {
        val content = compose.activity.findViewById<ViewGroup>(android.R.id.content)
        val bitmap = Bitmap.createBitmap(content.width, content.height, Bitmap.Config.ARGB_8888)
        content.draw(Canvas(bitmap))
        return bitmap.asImageBitmap().toPixelMap()
    }

    private fun androidx.compose.ui.graphics.PixelMap.pixelsNear(
        red: Float,
        green: Float,
        blue: Float,
        firstRow: Int = 0,
    ): Int = (firstRow until height).sumOf { y ->
        (0 until width).count { x ->
            val pixel = this[x, y]
            kotlin.math.abs(pixel.red - red) <= COLOUR_TOLERANCE &&
                kotlin.math.abs(pixel.green - green) <= COLOUR_TOLERANCE &&
                kotlin.math.abs(pixel.blue - blue) <= COLOUR_TOLERANCE
        }
    }

    private class VisualizerRenderData : TrackRenderDataPort {
        override val revision = 1
        var column: List<Int>? = null
        var lastPosition = Float.NaN
        var lastBarCount = 0
        var barAnswer: ((Int) -> List<TrackRenderBar>)? = { emptyList() }

        override fun bars(trackId: Long, barCount: Int): List<TrackRenderBar>? {
            lastBarCount = barCount
            return barAnswer?.invoke(barCount)
        }

        override fun spectrumColumn(trackId: Long, position: Float): List<Int>? {
            lastPosition = position
            return column
        }
    }

    private companion object {
        const val TRACK_ID = 73L
        const val BAND_COUNT = 24
        const val PLAYBACK_FRACTION = 0.625f
        const val FRACTION_TOLERANCE = 0.0001f
        const val VISUALIZER_SIZE_DP = 160
        const val COLOUR_TOLERANCE = 0.04f
        const val MINIMUM_BAND_PIXELS = 20
        const val PREVIEW_RED = 0.69f
        const val PREVIEW_GREEN = 0.26f
        const val PREVIEW_BLUE = 0.91f
        const val MINIMUM_PREVIEW_PIXELS = 100

        val bandColours = List(BAND_COUNT) { index ->
            val position = index.toFloat() / (BAND_COUNT - 1)
            SpectralBandColour(
                red = 0.92f - position * 0.76f,
                green = 0.14f + position * 0.68f,
                blue = 0.22f + position * 0.50f,
            )
        }
    }
}
