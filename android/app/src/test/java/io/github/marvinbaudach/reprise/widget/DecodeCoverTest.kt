package io.github.marvinbaudach.reprise.widget

import android.graphics.Bitmap
import androidx.compose.ui.unit.dp
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class DecodeCoverTest {
    @get:Rule
    val folder = TemporaryFolder()

    private fun png(width: Int, height: Int): File {
        val file = folder.newFile()
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        return file
    }

    @Test
    fun aLargeCoverIsDownscaledToTheWidgetLimit() {
        val cover = decodeCover(png(1_092, 1_092).path)!!

        assertTrue("${cover.width}x${cover.height}", maxOf(cover.width, cover.height) <= WIDGET_COVER_MAX_PX)
        assertTrue(cover.width > 0 && cover.height > 0)
    }

    @Test
    fun aNonSquareCoverKeepsItsProportions() {
        val cover = decodeCover(png(1_000, 500).path)!!

        assertTrue("${cover.width}x${cover.height}", maxOf(cover.width, cover.height) <= WIDGET_COVER_MAX_PX)
        assertTrue(cover.width.toFloat() / cover.height in 1.9f..2.1f)
    }

    @Test
    fun aSmallCoverIsLeftAlone() {
        val cover = decodeCover(png(100, 100).path)!!

        assertTrue(cover.width == 100 && cover.height == 100)
    }

    @Test
    fun oneCoverServesBothLayoutsAtTheLargerSlotsSize() {
        val file = png(1_092, 1_092)

        val cover = decodeWidgetCover(file.path, density = 2f)!!

        // The square slot is the larger one; the wide layout scales it down.
        assertEquals(coverPx(SQUARE_SIZE.width, 2f), cover.width)
    }

    @Test
    fun aDenseScreenNeverGetsMoreThanTheCap() {
        val cover = decodeWidgetCover(png(1_092, 1_092).path, density = 4f)!!

        assertTrue(cover.width <= WIDGET_COVER_MAX_PX)
    }

    /**
     * `RemoteViews` copies a mutable bitmap into shared memory on the main
     * thread, again for each layout; one that is already immutable and shared
     * is only handed on. Measured on a Pixel: that copy was 24-124 ms per
     * redraw.
     */
    @Test
    fun theCoverIsReadyToCrossTheBinderWithoutACopyOnTheMainThread() {
        val scaled = decodeWidgetCover(png(1_092, 1_092).path, density = 2f)!!
        val small = decodeWidgetCover(png(100, 100).path, density = 2f)!!

        assertFalse("a downscaled cover is mutable", scaled.isMutable)
        assertFalse("an untouched cover is mutable", small.isMutable)
    }

    @Test
    fun aMissingFileIsNoWidgetCover() {
        assertNull(decodeWidgetCover("/no/such/file.png", density = 2f))
    }

    @Test
    fun theSlotSizeRoundsUpAndIsCapped() {
        assertEquals(113, coverPx(56.dp, 2.01f))
        assertEquals(WIDGET_COVER_MAX_PX, coverPx(110.dp, 3f))
        assertEquals(1, coverPx(0.dp, 3f))
    }

    @Test
    fun aMissingFileIsNoCover() {
        assertNull(decodeCover("/no/such/file.png"))
    }
}
