package io.github.marvinbaudach.reprise

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Color
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.net.NetworkInfo
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.test.core.app.ApplicationProvider
import java.util.concurrent.atomic.AtomicInteger
import kotlin.coroutines.CoroutineContext
import kotlinx.coroutines.CoroutineDispatcher
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.shadows.ShadowNetworkCapabilities
import org.robolectric.shadows.ShadowNetworkInfo
import org.robolectric.shadows.ShadowLog
import uniffi.reprise_android_ffi.AndroidArtworkSize

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class NetworkReturnArtworkTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun net_7b_a_cover_that_failed_offline_is_fetched_when_the_network_returns() {
        val lanes = RetryArtworkLanes()
        val detector = NetworkReturnDetector()
        val cover = bitmap(Color.BLUE)
        var fetchSucceeds = false
        var coverAvailable = false
        val fetches = AtomicInteger()
        val artwork = TrackArtwork(
            resolve = { _, _ -> if (coverAvailable) COVER_PATH else null },
            resolveAlbumCoverFetched = { _, _ ->
                fetches.incrementAndGet()
                if (fetchSucceeds) {
                    coverAvailable = true
                    COVER_PATH
                } else {
                    null
                }
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
        detector.observe(online = false, artwork::networkReturned)

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
            lanes.runAll()
            compose.waitForIdle()
            assertEquals(true, mini?.generated)
            assertEquals(true, nowPlaying?.generated)
            assertEquals(1, fetches.get())

            fetchSucceeds = true
            ShadowLog.clear()
            compose.runOnIdle {
                detector.observe(online = true, artwork::networkReturned)
            }
            compose.waitForIdle()
            lanes.fullSize.runAll()
            compose.waitForIdle()
            lanes.runAll()
            compose.waitForIdle()

            assertEquals(2, fetches.get())
            assertSame(cover, nowPlaying?.image?.asAndroidBitmap())
            assertSame(cover, mini?.image?.asAndroidBitmap())
            assertTrue(
                ShadowLog.getLogsForTag(COVER_RETRY_TAG).any { item ->
                    item.type == Log.INFO &&
                        item.msg.contains(TRACK_URI) &&
                        item.msg.endsWith("hit")
                },
            )
        } finally {
            artwork.shutdown()
        }
    }

    @Test
    fun net_7b_a_real_cover_is_not_fetched_again_when_the_network_returns() {
        val lanes = RetryArtworkLanes()
        val detector = NetworkReturnDetector()
        val cover = bitmap(Color.GREEN)
        val fetches = AtomicInteger()
        val artwork = TrackArtwork(
            resolve = { _, _ -> COVER_PATH },
            resolveAlbumCoverFetched = { _, _ -> fetches.incrementAndGet(); COVER_PATH },
            decode = { path -> if (path == COVER_PATH) cover else null },
            cache = ArtworkCache(),
            fullSizeDispatcher = lanes.fullSize,
            onMainThread = { work -> work() },
        )
        var nowPlaying: ArtworkVisual? = null
        detector.observe(online = false, artwork::networkReturned)

        try {
            compose.setContent {
                CompositionLocalProvider(LocalTrackArtwork provides artwork) {
                    nowPlaying = rememberTrackArtworkVisual(
                        TRACK_URI,
                        AndroidArtworkSize.NOW_PLAYING,
                        allowFetch = true,
                    )
                }
            }
            lanes.fullSize.runAll()
            compose.waitForIdle()
            assertSame(cover, nowPlaying?.image?.asAndroidBitmap())

            compose.runOnIdle {
                detector.observe(online = true, artwork::networkReturned)
            }
            compose.waitForIdle()
            lanes.fullSize.runAll()
            compose.waitForIdle()

            assertEquals(0, fetches.get())
            assertSame(cover, nowPlaying?.image?.asAndroidBitmap())
        } finally {
            artwork.shutdown()
        }
    }

    @Test
    fun the_network_monitor_takes_an_offline_baseline_before_callbacks() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val manager = context.getSystemService(ConnectivityManager::class.java)
        val shadow = shadowOf(manager)
        shadow.setActiveNetworkInfo(
            ShadowNetworkInfo.newInstance(
                NetworkInfo.DetailedState.CONNECTED,
                ConnectivityManager.TYPE_WIFI,
                0,
                true,
                true,
            ),
        )
        val network = requireNotNull(manager.activeNetwork)
        shadow.setNetworkCapabilities(network, capabilities(validated = false))
        val detector = NetworkReturnDetector()
        var returns = 0
        val monitor = NetworkReturnMonitor(
            connectivity = manager,
            detector = detector,
            onNetworkReturned = { returns += 1 },
            postToMain = { work -> work() },
        )

        ShadowLog.clear()
        monitor.start()
        assertEquals(0, returns)
        assertEquals(1, shadow.networkCallbacks.size)

        shadow.networkCallbacks.single().onCapabilitiesChanged(
            network,
            capabilities(validated = true),
        )

        assertEquals(1, returns)
        assertTrue(
            ShadowLog.getLogsForTag(COVER_RETRY_TAG).any { item ->
                item.type == Log.INFO &&
                    item.msg.contains("wifi") &&
                    item.msg.contains("validated=true")
            },
        )
        monitor.stop()
        assertTrue(shadow.networkCallbacks.isEmpty())
    }

    private fun capabilities(validated: Boolean): NetworkCapabilities {
        val capabilities = ShadowNetworkCapabilities.newInstance()
        shadowOf(capabilities).apply {
            addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
            addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
            if (validated) addCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
        }
        return capabilities
    }

    private fun bitmap(colour: Int): Bitmap =
        Bitmap.createBitmap(8, 8, Bitmap.Config.ARGB_8888).apply { eraseColor(colour) }

    private companion object {
        const val TRACK_URI = "content://tracks/network-retry"
        const val COVER_PATH = "/covers/network-retry.jpg"
    }
}

private class RetryArtworkLanes {
    val list = RetryManualDispatcher()
    val fullSize = RetryManualDispatcher()

    fun runAll() {
        list.runAll()
        fullSize.runAll()
    }
}

private class RetryManualDispatcher : CoroutineDispatcher() {
    private val work = ArrayDeque<Runnable>()

    override fun dispatch(context: CoroutineContext, block: Runnable) {
        work.addLast(block)
    }

    fun runAll() {
        while (work.isNotEmpty()) work.removeFirst().run()
    }
}
