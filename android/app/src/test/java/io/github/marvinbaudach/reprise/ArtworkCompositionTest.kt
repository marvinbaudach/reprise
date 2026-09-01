package io.github.marvinbaudach.reprise

import android.graphics.Bitmap
import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import java.util.concurrent.Executors
import org.junit.Assert.assertSame
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.reprise_android_ffi.AndroidArtworkSize

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class ArtworkCompositionTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun cached_cover_is_non_null_in_the_first_composition() {
        val cache = ArtworkCache()
        val request = ArtworkRequest(
            trackUri = "content://tracks/cached-composition",
            size = AndroidArtworkSize.NOW_PLAYING,
            title = "Cached",
            artist = "Artist",
        )
        val expected = ArtworkVisual(
            Bitmap.createBitmap(4, 4, Bitmap.Config.ARGB_8888).asImageBitmap(),
            ambientColors = null,
        )
        cache.putArtwork(request, expected)
        val artwork = TrackArtwork(
            resolve = { _, _ -> error("cached artwork must not resolve again") },
            cache = cache,
        )
        var first: ArtworkVisual? = null

        try {
            compose.setContent {
                CompositionLocalProvider(LocalTrackArtwork provides artwork) {
                    first = rememberTrackArtworkVisual(
                        request.trackUri,
                        request.size,
                        request.title,
                        request.artist,
                    )
                }
            }
            compose.runOnIdle { assertSame(expected, first) }
        } finally {
            artwork.shutdown()
        }
    }

    /**
     * The incoming panel seeds itself from the prefetched list cover across
     * sizes. If the full-size read then finds nothing, the async delivery must
     * not replace that real cover with a generated one — that swap is what a
     * viewer sees as the cover flashing up and falling back to the note.
     */
    @Test
    @GraphicsMode(GraphicsMode.Mode.NATIVE)
    fun a_failed_full_size_read_keeps_the_cover_the_prefetch_already_warmed() {
        val cache = ArtworkCache()
        val prefetched = ArtworkRequest(
            trackUri = "content://tracks/incoming",
            size = AndroidArtworkSize.LIST,
            title = "Incoming",
            artist = "Artist",
        )
        val warmed = ArtworkVisual(
            Bitmap.createBitmap(4, 4, Bitmap.Config.ARGB_8888).asImageBitmap(),
            ambientColors = null,
        )
        cache.putArtwork(prefetched, warmed)
        val fullSize = Executors.newSingleThreadExecutor()
        // The list read succeeds and the full-size one does not: that asymmetry,
        // not a wholly unreadable track, is what puts a real cover on the shelf
        // and still sends the panel down the generated path.
        val artwork = TrackArtwork(
            resolve = { _, size ->
                if (size == AndroidArtworkSize.NOW_PLAYING) null else "/covers/incoming.jpg"
            },
            decode = { Bitmap.createBitmap(4, 4, Bitmap.Config.ARGB_8888) },
            cache = cache,
            fullSizeWorker = fullSize,
        )
        var shown: ArtworkVisual? = null

        try {
            compose.setContent {
                CompositionLocalProvider(LocalTrackArtwork provides artwork) {
                    shown = rememberTrackArtworkVisual(
                        prefetched.trackUri,
                        AndroidArtworkSize.NOW_PLAYING,
                        prefetched.title,
                        prefetched.artist,
                    )
                }
            }
            compose.waitForIdle()
            assertSame("the seed must cross sizes", warmed, shown)

            fullSize.submit { }.get()
            compose.waitForIdle()
            assertSame("the full-size read must not replace it", warmed, shown)
        } finally {
            artwork.shutdown()
            fullSize.shutdownNow()
        }
    }
}
