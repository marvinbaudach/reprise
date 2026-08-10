package de.reprise.spike

import android.graphics.Bitmap
import android.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.reprise_android_ffi.AndroidArtworkSize
import uniffi.reprise_android_ffi.AndroidFallbackCoverColours

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ArtworkCacheTest {
    @Test
    fun artwork_lru_evicts_the_oldest_entry_after_twelve_slots() {
        val cache = ArtworkCache(artworkCapacity = 2, fogCapacity = 1)
        val first = request("first")
        val second = request("second")
        val third = request("third")
        val firstVisual = visual(Color.RED)
        val secondVisual = visual(Color.GREEN)
        val thirdVisual = visual(Color.BLUE)

        cache.putArtwork(first, firstVisual)
        cache.putArtwork(second, secondVisual)
        assertSame(firstVisual, cache.artwork(first))
        cache.putArtwork(third, thirdVisual)

        assertSame(firstVisual, cache.artwork(first))
        assertNull(cache.artwork(second))
        assertSame(thirdVisual, cache.artwork(third))
    }

    @Test
    fun generated_cover_is_a_dark_gradient_and_never_the_old_teal_accent() {
        val bitmap = fallbackCoverBitmap(
            title = "No local image",
            artist = "An Artist",
            sizePx = 96,
            colours = AndroidFallbackCoverColours(top = 0x5a3322u, bottom = 0x241d35u),
        )

        val top = bitmap.getPixel(4, 4)
        val bottom = bitmap.getPixel(4, bitmap.height - 5)
        assertNotEquals(top, bottom)
        val oldTeal = Color.rgb(0, 150, 136) and 0x00ffffff
        assertNotEquals(oldTeal, top and 0x00ffffff)
        assertNotEquals(oldTeal, bottom and 0x00ffffff)
        assertEquals(255, Color.alpha(top))
    }

    private fun request(name: String) = ArtworkRequest(
        trackUri = "content://tracks/$name",
        size = AndroidArtworkSize.NOW_PLAYING,
        title = name,
        artist = "Artist",
    )

    private fun visual(colour: Int): ArtworkVisual {
        val bitmap = Bitmap.createBitmap(4, 4, Bitmap.Config.ARGB_8888).apply { eraseColor(colour) }
        return ArtworkVisual(bitmap.asImageBitmap(), ambientColors = null)
    }
}
