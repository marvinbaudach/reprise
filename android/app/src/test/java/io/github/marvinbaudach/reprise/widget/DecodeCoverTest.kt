package io.github.marvinbaudach.reprise.widget

import android.graphics.Bitmap
import java.io.File
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

        assertTrue("${cover.width}x${cover.height}", maxOf(cover.width, cover.height) <= WIDGET_COVER_PX)
        assertTrue(cover.width > 0 && cover.height > 0)
    }

    @Test
    fun aNonSquareCoverKeepsItsProportions() {
        val cover = decodeCover(png(1_000, 500).path)!!

        assertTrue("${cover.width}x${cover.height}", maxOf(cover.width, cover.height) <= WIDGET_COVER_PX)
        assertTrue(cover.width.toFloat() / cover.height in 1.9f..2.1f)
    }

    @Test
    fun aSmallCoverIsLeftAlone() {
        val cover = decodeCover(png(100, 100).path)!!

        assertTrue(cover.width == 100 && cover.height == 100)
    }

    @Test
    fun aMissingFileIsNoCover() {
        assertNull(decodeCover("/no/such/file.png"))
    }
}
