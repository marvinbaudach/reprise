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
    fun net_7b_a_fetch_that_fails_right_after_the_return_is_retried_by_a_follow_up() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val manager = context.getSystemService(ConnectivityManager::class.java)
        val shadow = shadowOf(manager)
        shadow.clearAllNetworks()
        shadow.setActiveNetworkInfo(null)
        val scheduler = ArtworkRetryScheduler()
        val lanes = RetryArtworkLanes()
        val downloaded = bitmap(Color.BLUE)
        val existing = bitmap(Color.GREEN)
        var online = false
        var coverAvailable = false
        val returnFetches = AtomicInteger()
        val realCoverFetches = AtomicInteger()
        val artwork = TrackArtwork(
            resolve = { uri, _ ->
                when {
                    uri == REAL_TRACK_URI -> REAL_COVER_PATH
                    coverAvailable -> COVER_PATH
                    else -> null
                }
            },
            resolveAlbumCoverFetched = { uri, _ ->
                if (uri == REAL_TRACK_URI) {
                    realCoverFetches.incrementAndGet()
                    REAL_COVER_PATH
                } else if (!online) {
                    null
                } else if (returnFetches.incrementAndGet() == 1) {
                    null
                } else {
                    coverAvailable = true
                    COVER_PATH
                }
            },
            decode = { path ->
                when (path) {
                    COVER_PATH -> downloaded
                    REAL_COVER_PATH -> existing
                    else -> null
                }
            },
            fallback = { _, _, _ -> bitmap(Color.MAGENTA) },
            cache = ArtworkCache(),
            dispatcher = lanes.list,
            fullSizeDispatcher = lanes.fullSize,
            onMainThread = { work -> work() },
        )
        val monitor = NetworkReturnMonitor(
            connectivity = manager,
            detector = NetworkReturnDetector(),
            onNetworkReturned = artwork::networkReturned,
            postToMain = { work -> work() },
            scheduler = scheduler,
        )
        var retrying: ArtworkVisual? = null
        var alreadyReal: ArtworkVisual? = null

        try {
            compose.setContent {
                CompositionLocalProvider(LocalTrackArtwork provides artwork) {
                    retrying = rememberTrackArtworkVisual(
                        TRACK_URI,
                        AndroidArtworkSize.NOW_PLAYING,
                        allowFetch = true,
                    )
                    alreadyReal = rememberTrackArtworkVisual(
                        REAL_TRACK_URI,
                        AndroidArtworkSize.NOW_PLAYING,
                        allowFetch = true,
                    )
                }
            }
            lanes.runAll()
            compose.waitForIdle()
            assertEquals(true, retrying?.generated)
            assertSame(existing, alreadyReal?.image?.asAndroidBitmap())

            monitor.start()
            online = true
            shadow.networkCallbacks.single().onCapabilitiesChanged(
                org.robolectric.shadows.ShadowNetwork.newInstance(60),
                capabilities(validated = true),
            )
            compose.waitForIdle()
            lanes.fullSize.runAll()
            compose.waitForIdle()
            assertEquals(true, retrying?.generated)

            scheduler.advanceBy(3_000L)
            compose.waitForIdle()
            lanes.fullSize.runAll()
            compose.waitForIdle()

            assertEquals(2, returnFetches.get())
            assertEquals(0, realCoverFetches.get())
            assertSame(downloaded, retrying?.image?.asAndroidBitmap())
            assertSame(existing, alreadyReal?.image?.asAndroidBitmap())
        } finally {
            monitor.stop()
            artwork.shutdown()
        }
    }

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
        val resolves = AtomicInteger()
        val artwork = TrackArtwork(
            resolve = { _, _ -> resolves.incrementAndGet(); COVER_PATH },
            resolveAlbumCoverFetched = { _, _ -> fetches.incrementAndGet(); COVER_PATH },
            decode = { path -> if (path == COVER_PATH) cover else null },
            fallback = { _, _, _ -> bitmap(Color.MAGENTA) },
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
            assertEquals(1, resolves.get())
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
                    item.msg.contains("validatedNonVpn=1")
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
            addCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN)
            if (validated) addCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
        }
        return capabilities
    }

    private fun bitmap(colour: Int): Bitmap =
        Bitmap.createBitmap(8, 8, Bitmap.Config.ARGB_8888).apply { eraseColor(colour) }

    private companion object {
        const val TRACK_URI = "content://tracks/network-retry"
        const val COVER_PATH = "/covers/network-retry.jpg"
        const val REAL_TRACK_URI = "content://tracks/already-real"
        const val REAL_COVER_PATH = "/covers/already-real.jpg"
    }
}

private class ArtworkRetryScheduler : NetworkReturnScheduler {
    private data class Scheduled(val dueAtMs: Long, val work: Runnable)

    private var nowMs = 0L
    private val scheduled = mutableListOf<Scheduled>()

    override fun postDelayed(work: Runnable, delayMs: Long) {
        scheduled += Scheduled(nowMs + delayMs, work)
    }

    override fun cancel(work: Runnable) {
        scheduled.removeAll { it.work === work }
    }

    fun advanceBy(delayMs: Long) {
        nowMs += delayMs
        val ready = scheduled.filter { it.dueAtMs <= nowMs }
        scheduled.removeAll(ready.toSet())
        ready.sortedBy(Scheduled::dueAtMs).forEach { it.work.run() }
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
