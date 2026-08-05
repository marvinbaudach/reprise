package de.reprise.spike

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.ViewGroup
import androidx.activity.ComponentActivity
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.unit.dp
import de.reprise.spike.ui.theme.RepriseTheme
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.reprise_android_ffi.AndroidColorScheme

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w200dp-h100dp")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class SeekBarPixelsTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val renderData = MutableRenderData()

    @Test
    fun unanalysedIsAFlatLineWhileAnalysedDataHasRealVerticalShape() {
        showBand()

        val flat = render()
        renderData.answer = { count ->
            List(count) { index ->
                TrackRenderBar(
                    silence = false,
                    level = if (index % 2 == 0) 1f else 0.2f,
                    red = 0.88f,
                    green = 0.18f,
                    blue = 0.31f,
                )
            }
        }
        renderData.revision += 1
        compose.waitForIdle()
        val analysed = render()

        val flatRows = flat.nonBackgroundRows()
        val analysedRows = analysed.nonBackgroundRows()
        assertTrue(
            "an unanalysed seek track must stay a plain line, but it painted $flatRows rows",
            flatRows <= MAXIMUM_FLAT_ROWS,
        )
        assertTrue(
            "analysed bars must have visible vertical shape, but they painted only $analysedRows rows",
            analysedRows >= MINIMUM_ANALYSED_ROWS,
        )
    }

    @Test
    fun silenceKeepsItsFixedDotWhileAnAudibleCellUsesItsLevel() {
        renderData.answer = { count ->
            List(count) { index ->
                TrackRenderBar(
                    silence = index % 2 == 0,
                    level = 1f,
                    red = if (index % 2 == 0) 0.92f else 0.08f,
                    green = if (index % 2 == 0) 0.12f else 0.86f,
                    blue = 0.18f,
                )
            }
        }
        showBand()

        val pixels = render()
        val silenceRows = pixels.rowsNear(red = 0.92f, green = 0.12f, blue = 0.18f)
        val audibleRows = pixels.rowsNear(red = 0.08f, green = 0.86f, blue = 0.18f)

        assertTrue(
            "silence must remain the fixed two-unit dot, but occupied ${silenceRows.size} rows",
            silenceRows.size in 1..MAXIMUM_SILENCE_ROWS,
        )
        assertTrue(
            "a full-level cell must be visibly tall, but occupied ${audibleRows.size} rows",
            audibleRows.size >= MINIMUM_AUDIBLE_ROWS,
        )
    }

    @Test
    fun barPixelsUseTheRgbValuesDeliveredWithTheRenderData() {
        renderData.answer = { count ->
            List(count) {
                TrackRenderBar(
                    silence = false,
                    level = 1f,
                    red = BOUNDARY_RED,
                    green = BOUNDARY_GREEN,
                    blue = BOUNDARY_BLUE,
                )
            }
        }
        showBand()

        val boundaryPixels = render().pixelsNear(
            red = BOUNDARY_RED,
            green = BOUNDARY_GREEN,
            blue = BOUNDARY_BLUE,
        )
        assertTrue(
            "the canvas must receive the boundary RGB instead of a theme colour, " +
                "but only $boundaryPixels pixels retained it",
            boundaryPixels >= MINIMUM_BOUNDARY_COLOUR_PIXELS,
        )
    }

    private fun showBand() {
        val theme = MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.SYSTEM,
            dynamicAvailable = false,
        )
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                CompositionLocalProvider(LocalTrackRenderData provides renderData) {
                    Box(Modifier.fillMaxSize().background(Color.Black)) {
                        SpectralTrackBand(
                            trackId = TRACK_ID,
                            modifier = Modifier.height(BAND_HEIGHT_DP.dp),
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

    private fun androidx.compose.ui.graphics.PixelMap.nonBackgroundRows(): Int =
        (0 until BAND_HEIGHT_DP).count { y ->
            (0 until width).any { x ->
                val pixel = this[x, y]
                pixel.red + pixel.green + pixel.blue > LIT_THRESHOLD
            }
        }

    private fun androidx.compose.ui.graphics.PixelMap.rowsNear(
        red: Float,
        green: Float,
        blue: Float,
    ): List<Int> = (0 until BAND_HEIGHT_DP).filter { y ->
        (0 until width).any { x -> this[x, y].near(red, green, blue) }
    }

    private fun androidx.compose.ui.graphics.PixelMap.pixelsNear(
        red: Float,
        green: Float,
        blue: Float,
    ): Int = (0 until BAND_HEIGHT_DP).sumOf { y ->
        (0 until width).count { x -> this[x, y].near(red, green, blue) }
    }

    private fun Color.near(red: Float, green: Float, blue: Float): Boolean =
        kotlin.math.abs(this.red - red) <= COLOUR_TOLERANCE &&
            kotlin.math.abs(this.green - green) <= COLOUR_TOLERANCE &&
            kotlin.math.abs(this.blue - blue) <= COLOUR_TOLERANCE

    private class MutableRenderData : TrackRenderDataPort {
        override var revision by mutableIntStateOf(0)
        var answer: ((Int) -> List<TrackRenderBar>)? = null

        override fun bars(trackId: Long, barCount: Int): List<TrackRenderBar>? =
            answer?.invoke(barCount)

        override fun spectrumColumn(trackId: Long, position: Float): List<Int>? = null
    }

    private companion object {
        const val TRACK_ID = 41L
        const val BAND_HEIGHT_DP = 32
        const val MAXIMUM_FLAT_ROWS = 5
        const val MINIMUM_ANALYSED_ROWS = 24
        const val MAXIMUM_SILENCE_ROWS = 4
        const val MINIMUM_AUDIBLE_ROWS = 24
        const val LIT_THRESHOLD = 0.15f
        const val COLOUR_TOLERANCE = 0.04f
        const val BOUNDARY_RED = 0.73f
        const val BOUNDARY_GREEN = 0.21f
        const val BOUNDARY_BLUE = 0.84f
        const val MINIMUM_BOUNDARY_COLOUR_PIXELS = 300
    }
}
