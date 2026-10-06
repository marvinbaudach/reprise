package io.github.marvinbaudach.reprise.widget

import android.graphics.Bitmap
import androidx.compose.ui.unit.dp
import java.io.File
import org.junit.Assert.assertEquals
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
    fun aCoverIsDecodedNoLargerThanItsSlotNeeds() {
        val file = png(1_092, 1_092)

        val wide = decodeCovers(file.path, density = 2f).wide!!
        val square = decodeCovers(file.path, density = 2f).square!!

        assertTrue("${wide.width}px", wide.width <= 112)
        assertTrue("${square.width}px", square.width <= WIDGET_COVER_MAX_PX)
        assertTrue(wide.width < square.width)
    }

    @Test
    fun aDenseScreenNeverGetsMoreThanTheCap() {
        val file = png(1_092, 1_092)

        val covers = decodeCovers(file.path, density = 4f)

        assertTrue(covers.wide!!.width <= WIDGET_COVER_MAX_PX)
        assertTrue(covers.square!!.width <= WIDGET_COVER_MAX_PX)
    }

    @Test
    fun theSlotSizeRoundsUpAndIsCapped() {
        assertEquals(113, coverPx(56.dp, 2.01f))
        assertEquals(WIDGET_COVER_MAX_PX, coverPx(110.dp, 3f))
        assertEquals(1, coverPx(0.dp, 3f))
    }

    @Test
    fun theWideLayoutGetsItsOwnBitmapAndTheSquareItsOwn() {
        val covers = WidgetCovers(
            wide = Bitmap.createBitmap(10, 10, Bitmap.Config.ARGB_8888),
            square = Bitmap.createBitmap(20, 20, Bitmap.Config.ARGB_8888),
        )

        assertEquals(10, covers.forSize(WIDE_SIZE)!!.width)
        assertEquals(20, covers.forSize(SQUARE_SIZE)!!.width)
    }

    @Test
    fun aMissingFileIsNoCover() {
        assertNull(decodeCover("/no/such/file.png"))
    }
}
