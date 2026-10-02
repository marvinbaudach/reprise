package io.github.marvinbaudach.reprise

import android.graphics.Bitmap
import android.graphics.Color
import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import java.util.concurrent.atomic.AtomicInteger
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

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class AlbumCoverLiveRefreshTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun net_7a_a_cover_now_playing_downloads_reaches_the_mini_player() {
        val lanes = ArtworkLanes()
        val cover = bitmap(Color.BLUE)
        var coverAvailable = false
        val fetches = AtomicInteger()
        val artwork = TrackArtwork(
            resolve = { _, _ -> if (coverAvailable) COVER_PATH else null },
            resolveAlbumCoverFetched = { _, _ ->
                fetches.incrementAndGet()
                coverAvailable = true
                COVER_PATH
            },
            decode = { path -> if (path == COVER_PATH) cover else null },
            fallback = { _, _, _ -> bitmap(Color.MAGENTA) },
            cache = ArtworkCache(),
            dispatcher = lanes.list,
            fullSizeDispatcher = lanes.fullSize,
            onMainThread = { work -> work() },
        )
        var mini: ArtworkVisual? = null
        var nowPlaying: ArtworkVisual? = null

        try {
            compose.setContent {
                CompositionLocalProvider(LocalTrackArtwork provides artwork) {
                    mini = rememberTrackArtworkVisual(TRACK_URI, AndroidArtworkSize.LIST)
                    nowPlaying = rememberTrackArtworkVisual(
                        TRACK_URI,
                        AndroidArtworkSize.NOW_PLAYING,
                        allowFetch = true,
                    )
                }
            }
            lanes.list.runAll()
            compose.waitForIdle()
            assertEquals(true, mini?.generated)

            lanes.fullSize.runAll()
            compose.waitForIdle()
            lanes.runAll()
            compose.waitForIdle()

            assertSame(cover, mini?.image?.asAndroidBitmap())
            assertSame(cover, nowPlaying?.image?.asAndroidBitmap())
            assertEquals(1, fetches.get())
        } finally {
            artwork.shutdown()
        }
    }

    @Test
    fun net_7a_a_cover_reaches_rows_of_other_tracks_on_the_album() {
        val lanes = ArtworkLanes()
        val cover = bitmap(Color.GREEN)
        var coverAvailable = false
        val rowReads = AtomicInteger()
        val session = LibrarySession(fakeLibrarySessionPort(
            artworkFor = { trackUri, _ ->
                if (trackUri == OTHER_TRACK_URI) rowReads.incrementAndGet()
                if (coverAvailable) COVER_PATH else null
            },
            artworkFetched = { _, _ ->
                coverAvailable = true
                COVER_PATH
            },
        ))
        val artwork = TrackArtwork(
            resolve = session::artworkFor,
            resolveAlbumCoverFetched = session::artworkFetched,
            forgetAlbumArtworkMisses = session::forgetArtworkMisses,
            decode = { path -> if (path == COVER_PATH) cover else null },
            fallback = { _, _, _ -> bitmap(Color.MAGENTA) },
            cache = ArtworkCache(),
            dispatcher = lanes.list,
            fullSizeDispatcher = lanes.fullSize,
            onMainThread = { work -> work() },
        )
        var otherRow: ArtworkVisual? = null

        try {
            compose.setContent {
                CompositionLocalProvider(LocalTrackArtwork provides artwork) {
                    otherRow = rememberTrackArtworkVisual(
                        OTHER_TRACK_URI,
                        AndroidArtworkSize.LIST,
                    )
                    rememberTrackArtworkVisual(
                        TRACK_URI,
                        AndroidArtworkSize.NOW_PLAYING,
                        allowFetch = true,
                    )
                }
            }
            lanes.list.runAll()
            compose.waitForIdle()
            assertEquals(true, otherRow?.generated)
            assertEquals(1, rowReads.get())

            lanes.fullSize.runAll()
            compose.waitForIdle()
            lanes.runAll()
            compose.waitForIdle()

            assertSame(cover, otherRow?.image?.asAndroidBitmap())
            assertEquals(2, rowReads.get())
        } finally {
            artwork.shutdown()
        }
    }

    @Test
    fun net_7a_a_cover_reaches_a_now_playing_surface_that_does_not_fetch() {
        val lanes = ArtworkLanes()
        val cover = bitmap(Color.CYAN)
        var coverAvailable = false
        val artwork = TrackArtwork(
            resolve = { _, _ -> if (coverAvailable) COVER_PATH else null },
            resolveAlbumCoverFetched = { _, _ ->
                coverAvailable = true
                COVER_PATH
            },
            decode = { path -> if (path == COVER_PATH) cover else null },
            fallback = { _, _, _ -> bitmap(Color.MAGENTA) },
            cache = ArtworkCache(),
            dispatcher = lanes.list,
            fullSizeDispatcher = lanes.fullSize,
            onMainThread = { work -> work() },
        )
        var sheet: ArtworkVisual? = null

        try {
            compose.setContent {
                CompositionLocalProvider(LocalTrackArtwork provides artwork) {
                    sheet = rememberTrackArtworkVisual(
                        TRACK_URI,
                        AndroidArtworkSize.NOW_PLAYING,
                        allowFetch = false,
                    )
                    rememberTrackArtworkVisual(
                        TRACK_URI,
                        AndroidArtworkSize.ARTIST_DETAIL,
                        allowFetch = true,
                    )
                }
            }
            lanes.fullSize.runAll()
            compose.waitForIdle()
            assertEquals(true, sheet?.generated)

            lanes.fullSize.runAll()
            compose.waitForIdle()

            assertSame(cover, sheet?.image?.asAndroidBitmap())
        } finally {
            artwork.shutdown()
        }
    }

    @Test
    fun net_7a_a_real_cover_is_not_read_again() {
        val lanes = ArtworkLanes()
        val cover = bitmap(Color.YELLOW)
        val realReads = AtomicInteger()
        val artwork = TrackArtwork(
            resolve = { trackUri, _ ->
                if (trackUri == REAL_TRACK_URI) {
                    realReads.incrementAndGet()
                    REAL_PATH
                } else {
                    null
                }
            },
            resolveAlbumCoverFetched = { _, _ -> COVER_PATH },
            decode = { path -> if (path == REAL_PATH || path == COVER_PATH) cover else null },
            fallback = { _, _, _ -> bitmap(Color.MAGENTA) },
            cache = ArtworkCache(),
            dispatcher = lanes.list,
            fullSizeDispatcher = lanes.fullSize,
            onMainThread = { work -> work() },
        )
        var real: ArtworkVisual? = null

        try {
            compose.setContent {
                CompositionLocalProvider(LocalTrackArtwork provides artwork) {
                    real = rememberTrackArtworkVisual(REAL_TRACK_URI, AndroidArtworkSize.LIST)
                    rememberTrackArtworkVisual(
                        TRACK_URI,
                        AndroidArtworkSize.NOW_PLAYING,
                        allowFetch = true,
                    )
                }
            }
            lanes.list.runAll()
            compose.waitForIdle()
            assertSame(cover, real?.image?.asAndroidBitmap())

            lanes.fullSize.runAll()
            compose.waitForIdle()
            lanes.runAll()
            compose.waitForIdle()

            assertEquals(1, realReads.get())
            assertSame(cover, real?.image?.asAndroidBitmap())
        } finally {
            artwork.shutdown()
        }
    }

    @Test
    fun a_surface_entering_after_an_album_bump_resolves_only_once() {
        val lanes = ArtworkLanes()
        val lateReads = AtomicInteger()
        var showLate by mutableStateOf(false)
        val artwork = TrackArtwork(
            resolve = { trackUri, _ ->
                if (trackUri == OTHER_TRACK_URI) lateReads.incrementAndGet()
                null
            },
            resolveAlbumCoverFetched = { _, _ -> COVER_PATH },
            decode = { path -> if (path == COVER_PATH) bitmap(Color.BLUE) else null },
            fallback = { _, _, _ -> bitmap(Color.MAGENTA) },
            cache = ArtworkCache(),
            dispatcher = lanes.list,
            fullSizeDispatcher = lanes.fullSize,
            onMainThread = { work -> work() },
        )

        try {
            compose.setContent {
                CompositionLocalProvider(LocalTrackArtwork provides artwork) {
                    rememberTrackArtworkVisual(
                        TRACK_URI,
                        AndroidArtworkSize.NOW_PLAYING,
                        allowFetch = true,
                    )
                    if (showLate) {
                        rememberTrackArtworkVisual(OTHER_TRACK_URI, AndroidArtworkSize.LIST)
                    }
                }
            }
            lanes.fullSize.runAll()
            compose.waitForIdle()

            showLate = true
            compose.waitForIdle()
            lanes.list.runAll()
            compose.waitForIdle()

            assertEquals(1, lateReads.get())
        } finally {
            artwork.shutdown()
        }
    }

    private fun bitmap(colour: Int): Bitmap =
        Bitmap.createBitmap(8, 8, Bitmap.Config.ARGB_8888).apply { eraseColor(colour) }

    private companion object {
        const val TRACK_URI = "content://tracks/downloader"
        const val OTHER_TRACK_URI = "content://tracks/same-album"
        const val REAL_TRACK_URI = "content://tracks/already-real"
        const val COVER_PATH = "/covers/downloaded.jpg"
        const val REAL_PATH = "/covers/local.jpg"
    }
}

private class ArtworkLanes {
    val list = ArtworkManualDispatcher()
    val fullSize = ArtworkManualDispatcher()

    fun runAll() {
        list.runAll()
        fullSize.runAll()
    }
}

private class ArtworkManualDispatcher : CoroutineDispatcher() {
    private val work = ArrayDeque<Runnable>()

    override fun dispatch(context: CoroutineContext, block: Runnable) {
        work.addLast(block)
    }

    fun runAll() {
        while (work.isNotEmpty()) work.removeFirst().run()
    }
}
