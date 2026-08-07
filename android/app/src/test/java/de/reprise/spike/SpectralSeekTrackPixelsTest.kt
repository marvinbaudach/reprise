package de.reprise.spike

import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.ViewGroup
import androidx.activity.ComponentActivity
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PixelMap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
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

/** The analysis claim is pixels: bars and plain fallback must really paint differently. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h200dp")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class SpectralSeekTrackPixelsTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val analysis = PixelAnalysis()
    private var directPlain by mutableStateOf(false)

    @Test
    fun anImportedAnalysisPaintsDifferentPixelsFromThePlainBar() {
        showTrack()
        val plain = render()

        analysis.answer(
            listOf(
                SpectralBar(false, 0.2f, 1.0, 0.0, 0.0),
                SpectralBar(false, 1.0f, 0.0, 1.0, 0.0),
            ),
        )
        compose.waitForIdle()
        val spectral = render()

        assertTrue("analysis changed no drawn seek pixels", plain.differenceCount(spectral) > 40)
        val redInk = spectral.colouredPixels { pixel ->
            pixel.red > pixel.green * 1.5f && pixel.red > pixel.blue * 1.5f
        }
        val greenInk = spectral.colouredPixels { pixel ->
            pixel.green > pixel.red * 1.5f && pixel.green > pixel.blue * 1.5f
        }
        assertTrue("the short Rust-owned red bar was not painted", redInk > 0)
        assertTrue("the tall Rust-owned green bar was not painted", greenInk > redInk * 2)
    }

    @Test
    fun noAnalysisIsPixelForPixelTheExistingPlainTrack() {
        showTrack()
        val noAnalysis = render()

        directPlain = true
        compose.waitForIdle()
        val existingPlain = render()

        assertEquals(0, noAnalysis.differenceCount(existingPlain))
    }

    @Test
    fun playedBarsPaintMoreStronglyThanBarsStillToCome() {
        showTrack(positionMs = 60_000)
        analysis.answer(List(80) { SpectralBar(false, 0.8f, 0.2, 0.7, 1.0) })
        compose.waitForIdle()

        val pixels = render()
        val leftInk = pixels.blueInk(0, pixels.width / 2)
        val rightInk = pixels.blueInk(pixels.width / 2, pixels.width)
        assertTrue(
            "played and remaining bars painted with indistinguishable strength: $leftInk vs $rightInk",
            leftInk > rightInk * 1.35f,
        )
    }

    private fun showTrack(positionMs: Long = 120_000) {
        val theme = MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.SYSTEM,
            dynamicAvailable = false,
        )
        compose.setContent {
            RepriseTheme(theme, darkPalette = true) {
                Box(Modifier.fillMaxSize().background(Color.Black)) {
                    if (directPlain) {
                        PlainSeekTrack(positionMs, 120_000)
                    } else {
                        CompositionLocalProvider(LocalTrackAnalysis provides analysis) {
                            SpectralSeekTrack(9, positionMs, 120_000)
                        }
                    }
                }
            }
        }
        compose.waitForIdle()
    }

    private fun render(): PixelMap {
        val content = compose.activity.findViewById<ViewGroup>(android.R.id.content)
        val bitmap = Bitmap.createBitmap(content.width, content.height, Bitmap.Config.ARGB_8888)
        content.draw(Canvas(bitmap))
        return bitmap.asImageBitmap().toPixelMap()
    }

    private fun PixelMap.differenceCount(other: PixelMap): Int =
        (0 until height).sumOf { y ->
            (0 until width).count { x -> this[x, y] != other[x, y] }
        }

    private fun PixelMap.colouredPixels(predicate: (Color) -> Boolean): Int =
        (0 until height).sumOf { y -> (0 until width).count { x -> predicate(this[x, y]) } }

    private fun PixelMap.blueInk(fromX: Int, untilX: Int): Float =
        (0 until height).sumOf { y ->
            (fromX until untilX).sumOf { x -> this[x, y].blue.toDouble() }
        }.toFloat()
}

private class PixelAnalysis : TrackAnalysisPort {
    private var bars: List<SpectralBar>? = null
    override var revision by mutableLongStateOf(0L)
        private set

    fun answer(answer: List<SpectralBar>?) {
        bars = answer
        revision += 1L
    }

    override fun prepare(trackId: Long) = Unit
    override fun loadBars(trackId: Long, count: Int, deliver: (List<SpectralBar>?) -> Unit) {
        deliver(bars)
    }
}
