package io.github.marvinbaudach.reprise

import android.graphics.Bitmap
import android.graphics.Color
import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import kotlin.coroutines.CoroutineContext
import kotlinx.coroutines.CoroutineDispatcher
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.reprise_android_ffi.AndroidArtworkSize

/**
 * #998: a now-playing view that is already open keeps its placeholder when the
 * cover lands afterwards.
 *
 * `AlbumCoverLiveRefreshTest` pins the downloads this view starts itself. This
 * one pins the other half of the issue: the view's own request gave up empty
 * and the cover reached the disk later, through the backfill. Only the
 * backfill's `albumCoversChanged()` tells the view.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class TrackCoverLandsAfterTheViewTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun net_7a_an_open_now_playing_view_switches_to_a_cover_that_lands_after_its_own_request_gave_up() {
        val lane = ManualLane()
        val cover = Bitmap.createBitmap(8, 8, Bitmap.Config.ARGB_8888).apply { eraseColor(Color.RED) }
        var onDisk = false
        val artwork = TrackArtwork(
            resolve = { _, _ -> if (onDisk) COVER_PATH else null },
            // The view's own fetch ran into its timeout.
            resolveAlbumCoverFetched = { _, _ -> null },
            decode = { path -> if (path == COVER_PATH) cover else null },
            fallback = { _, _, _ ->
                Bitmap.createBitmap(8, 8, Bitmap.Config.ARGB_8888).apply { eraseColor(Color.MAGENTA) }
            },
            cache = ArtworkCache(),
            dispatcher = lane,
            fullSizeDispatcher = lane,
            onMainThread = { work -> work() },
        )
        var shown: ArtworkVisual? = null
        try {
            compose.setContent {
                CompositionLocalProvider(LocalTrackArtwork provides artwork) {
                    shown = rememberTrackArtworkVisual(
                        TRACK_URI,
                        AndroidArtworkSize.NOW_PLAYING,
                        allowFetch = true,
                    )
                }
            }
            lane.runAll()
            compose.waitForIdle()
            assertEquals(true, shown?.generated)

            // The backfill stores the cover, then reports the download.
            onDisk = true
            compose.runOnIdle { artwork.albumCoversChanged() }
            compose.waitForIdle()
            lane.runAll()
            compose.waitForIdle()

            assertEquals(false, shown?.generated)
            assertSame(cover, shown?.image?.asAndroidBitmap())
        } finally {
            artwork.shutdown()
        }
    }

    private class ManualLane : CoroutineDispatcher() {
        private val work = ArrayDeque<Runnable>()

        override fun dispatch(context: CoroutineContext, block: Runnable) {
            work.addLast(block)
        }

        fun runAll() {
            while (work.isNotEmpty()) work.removeFirst().run()
        }
    }

    private companion object {
        const val TRACK_URI = "content://tracks/late-cover"
        const val COVER_PATH = "/covers/late.jpg"
    }
}
