package io.github.marvinbaudach.reprise

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
import io.github.marvinbaudach.reprise.scene.SpectrogramFrames
import io.github.marvinbaudach.reprise.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.reprise_android_ffi.AndroidColorScheme

private const val MAX_FRAMES_TO_SWAP = 12

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
    fun nav_15d_partial_bars_cover_only_the_decoded_part() {
        showTrack(positionMs = 0)
        val plain = render()

        analysis.partial(
            PartialTrackAnalysis(
                coveredFraction = 0.5f,
                bars = List(40) { SpectralBar(false, 0.9f, 1.0, 0.0, 0.0) },
                frames = SpectrogramFrames(bandCount = 2, frameRateHz = 20, cells = byteArrayOf()),
            ),
        )
        compose.waitForIdle()
        val partial = render()

        val half = partial.width / 2
        assertTrue(
            "the decoded left half painted no bars",
            partial.redInk(0, half) > 40 && plain.redInk(0, half) == 0,
        )
        assertEquals(
            "the undecoded right half must stay the plain line",
            0,
            plain.differenceCount(partial, fromX = half + 1, untilX = partial.width),
        )
    }

    @Test
    fun nav_15d_final_bars_replace_the_partial() {
        // A cue revision makes a fresh bar swap build in; the swap from a partial must not.
        showTrack(positionMs = 0, cueRevision = 1)
        analysis.partial(
            PartialTrackAnalysis(
                coveredFraction = 0.5f,
                bars = List(40) { SpectralBar(false, 0.9f, 1.0, 0.0, 0.0) },
                frames = SpectrogramFrames(bandCount = 2, frameRateHz = 20, cells = byteArrayOf()),
            ),
        )
        compose.waitForIdle()
        assertTrue("the partial never painted", render().redInk(0, 250) > 40)

        compose.mainClock.autoAdvance = false
        analysis.answer(List(80) { SpectralBar(false, 0.9f, 0.0, 1.0, 0.0) })
        var swapped: PixelMap? = null
        repeat(MAX_FRAMES_TO_SWAP) {
            if (swapped == null) {
                compose.mainClock.advanceTimeByFrame()
                render().takeIf { it.redInk(0, it.width) == 0 }?.let { swapped = it }
            }
        }
        assertTrue("the final bars never replaced the partial", swapped != null)
        repeat(2) { compose.mainClock.advanceTimeByFrame() }
        val afterEffects = render()
        compose.mainClock.autoAdvance = true
        compose.waitForIdle()
        val final = render()

        assertEquals(
            "the final bars grew in again instead of replacing the partial at full height",
            0,
            swapped!!.differenceCount(final),
        )
        assertEquals(0, afterEffects.differenceCount(final))

        assertEquals("the partial bars were still painted", 0, final.redInk(0, final.width))
        assertTrue(
            "the final bars did not span the right half",
            final.greenInk(final.width / 2, final.width) > 40,
        )
    }

    @Test
    fun nav_15d_a_poll_before_the_revision_bump_keeps_the_partial() {
        showTrack(positionMs = 0)
        analysis.partial(halfDecoded())
        compose.waitForIdle()
        assertTrue("the partial never painted", render().redInk(0, 250) > 40)

        // The decode stored its result: the poll sees nothing, the revision has not moved yet.
        compose.mainClock.autoAdvance = false
        analysis.dropProgressWithoutRevision()
        compose.mainClock.advanceTimeBy(2 * ANALYSIS_PROGRESS_POLL_MS)

        assertTrue("the partial flashed away", render().redInk(0, 250) > 40)
    }

    @Test
    fun nav_15d_a_decode_that_ends_without_a_result_clears_the_partial() {
        showTrack(positionMs = 0)
        analysis.partial(halfDecoded())
        compose.waitForIdle()
        assertTrue("the partial never painted", render().redInk(0, 250) > 40)

        analysis.partial(null)
        compose.waitForIdle()

        assertEquals(0, render().redInk(0, 500))
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

    @Test
    fun played_fraction_has_a_bright_vertical_marker_without_analysis() {
        showTrack(positionMs = 60_000)

        val pixels = render()
        val markerX = pixels.width / 2
        val brightMarkerPixels = (0 until pixels.height).count { y ->
            val pixel = pixels[markerX, y]
            pixel.red > 0.75f && pixel.green > 0.75f && pixel.blue > 0.75f
        }

        assertTrue("the played fraction has no full-height accent marker", brightMarkerPixels >= 20)
    }

    private fun halfDecoded() = PartialTrackAnalysis(
        coveredFraction = 0.5f,
        bars = List(40) { SpectralBar(false, 0.9f, 1.0, 0.0, 0.0) },
        frames = SpectrogramFrames(bandCount = 2, frameRateHz = 20, cells = byteArrayOf()),
    )

    private fun showTrack(positionMs: Long = 120_000, cueRevision: Int = 0) {
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
                            SpectralSeekTrack(9, positionMs, 120_000, cueRevision)
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

    private fun PixelMap.differenceCount(
        other: PixelMap,
        fromX: Int = 0,
        untilX: Int = width,
    ): Int =
        (0 until height).sumOf { y ->
            (fromX until untilX).count { x -> this[x, y] != other[x, y] }
        }

    private fun PixelMap.redInk(fromX: Int, untilX: Int): Int =
        (0 until height).sumOf { y ->
            (fromX until untilX).count { x ->
                this[x, y].red > this[x, y].green * 1.5f && this[x, y].red > this[x, y].blue * 1.5f
            }
        }

    private fun PixelMap.greenInk(fromX: Int, untilX: Int): Int =
        (0 until height).sumOf { y ->
            (fromX until untilX).count { x ->
                this[x, y].green > this[x, y].red * 1.5f && this[x, y].green > this[x, y].blue * 1.5f
            }
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
    private var progress: PartialTrackAnalysis? = null
    override var revision by mutableLongStateOf(0L)
        private set

    fun answer(answer: List<SpectralBar>?) {
        bars = answer
        revision += 1L
    }

    fun partial(answer: PartialTrackAnalysis?) {
        progress = answer
        revision += 1L
    }

    fun dropProgressWithoutRevision() {
        progress = null
    }

    override fun prepare(trackId: Long) = Unit
    override fun loadProgress(
        trackId: Long,
        count: Int,
        deliver: (PartialTrackAnalysis?) -> Unit,
    ) = deliver(progress)

    override fun loadBars(trackId: Long, count: Int, deliver: (List<SpectralBar>?) -> Unit) {
        deliver(bars)
    }
}
