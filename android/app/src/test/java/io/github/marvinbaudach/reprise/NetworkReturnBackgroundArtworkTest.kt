package io.github.marvinbaudach.reprise

import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import androidx.test.core.app.ApplicationProvider
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowNetwork
import org.robolectric.shadows.ShadowNetworkCapabilities

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class NetworkReturnBackgroundArtworkTest {
    @Test
    fun net_7d_one_real_network_return_starts_one_background_pass_not_followups() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val manager = context.getSystemService(ConnectivityManager::class.java)
        val shadow = shadowOf(manager)
        shadow.clearAllNetworks()
        shadow.setActiveNetworkInfo(null)
        val scheduler = RealReturnScheduler()
        val surface = MobileSurfaceViewModel()
        var starts = 0
        var visibleRetries = 0
        surface.bindArtistPhotoBackfill(
            snapshot = ::idleArtworkProgress,
            start = { starts += 1 },
            cancel = {},
        )
        val monitor = NetworkReturnMonitor(
            connectivity = manager,
            detector = NetworkReturnDetector(),
            onNetworkReturned = { visibleRetries += 1 },
            onRealNetworkReturned = surface::networkReturnedRestartArtwork,
            postToMain = { work -> work() },
            scheduler = scheduler,
        )

        monitor.start()
        shadow.networkCallbacks.single().onCapabilitiesChanged(
            ShadowNetwork.newInstance(80),
            validatedWifiCapabilities(),
        )
        scheduler.runAll()

        assertEquals(1, starts)
        assertEquals(4, visibleRetries)
        monitor.stop()
    }

    @Test
    fun stoppedArtworkWaitsForAScanBeforeNetworkReturnsCanRestartIt() {
        val surface = MobileSurfaceViewModel()
        var starts = 0
        var cancels = 0
        surface.bindArtistPhotoBackfill(
            snapshot = ::idleArtworkProgress,
            start = { starts += 1 },
            cancel = { cancels += 1 },
        )

        surface.cancelArtistPhotoBackfill()
        surface.networkReturnedRestartArtwork()

        assertEquals(1, cancels)
        assertEquals(0, starts)

        surface.scanCompletedRestartArtwork()
        surface.networkReturnedRestartArtwork()

        assertEquals(2, starts)
    }
}

private fun idleArtworkProgress() = ArtistPhotoProgress(
    runId = 0,
    phase = ArtistPhotoProgressPhase.COMPLETE,
    done = 0,
    failed = 0,
    total = 0,
)

private fun validatedWifiCapabilities(): NetworkCapabilities =
    ShadowNetworkCapabilities.newInstance().also { capabilities ->
        shadowOf(capabilities).apply {
            addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
            addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
            addCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN)
            addCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
        }
    }

private class RealReturnScheduler : NetworkReturnScheduler {
    private val pending = ArrayDeque<Runnable>()

    override fun postDelayed(work: Runnable, delayMs: Long) {
        pending += work
    }

    override fun cancel(work: Runnable) {
        pending.remove(work)
    }

    fun runAll() {
        while (pending.isNotEmpty()) pending.removeFirst().run()
    }
}
