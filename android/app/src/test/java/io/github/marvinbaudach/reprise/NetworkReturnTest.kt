package io.github.marvinbaudach.reprise

import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.net.NetworkInfo
import androidx.test.core.app.ApplicationProvider
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowConnectivityManager
import org.robolectric.shadows.ShadowNetworkCapabilities
import org.robolectric.shadows.ShadowNetworkInfo

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class NetworkReturnTest {
    @Test
    fun net_7b_an_offline_to_online_transition_reports_one_network_return() {
        val detector = NetworkReturnDetector()

        assertFalse(detector.observe(online = false))
        assertTrue(detector.observe(online = true))
        assertFalse(detector.observe(online = true))
    }

    @Test
    fun a_cold_start_without_an_active_network_reports_the_first_validated_network() {
        val fixture = monitorFixture(activeValidated = null)

        fixture.monitor.start()
        assertEquals(0, fixture.returns())

        val callback = fixture.shadow.networkCallbacks.single()
        val network = requireNotNull(onlineNetwork(fixture.connectivity, validated = true))
        callback.onAvailable(network)
        callback.onCapabilitiesChanged(network, capabilities(validated = true))

        assertEquals(1, fixture.returns())
        fixture.monitor.stop()
    }

    @Test
    fun switching_between_validated_default_networks_is_not_a_return() {
        val fixture = monitorFixture(activeValidated = true)

        fixture.monitor.start()
        val callback = fixture.shadow.networkCallbacks.single()
        val secondNetwork = requireNotNull(onlineNetwork(fixture.connectivity, validated = true))
        callback.onAvailable(secondNetwork)
        callback.onCapabilitiesChanged(secondNetwork, capabilities(validated = true))

        assertEquals(0, fixture.returns())
        fixture.monitor.stop()
    }

    @Test
    fun a_return_while_stopped_is_reported_on_the_next_start() {
        val fixture = monitorFixture(activeValidated = null)

        fixture.monitor.start()
        fixture.monitor.stop()
        onlineNetwork(fixture.connectivity, validated = true)
        fixture.monitor.start()

        assertEquals(1, fixture.returns())
        fixture.monitor.stop()
    }

    private fun monitorFixture(activeValidated: Boolean?): MonitorFixture {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val connectivity = context.getSystemService(ConnectivityManager::class.java)
        val shadow = shadowOf(connectivity)
        shadow.clearAllNetworks()
        shadow.setActiveNetworkInfo(null)
        if (activeValidated != null) onlineNetwork(connectivity, activeValidated)
        var returns = 0
        val monitor = NetworkReturnMonitor(
            connectivity = connectivity,
            detector = NetworkReturnDetector(),
            onNetworkReturned = { returns += 1 },
            postToMain = { work -> work() },
        )
        return MonitorFixture(connectivity, shadow, monitor) { returns }
    }

    private fun onlineNetwork(
        connectivity: ConnectivityManager,
        validated: Boolean,
    ) = shadowOf(connectivity).run {
        setActiveNetworkInfo(
            ShadowNetworkInfo.newInstance(
                NetworkInfo.DetailedState.CONNECTED,
                ConnectivityManager.TYPE_WIFI,
                0,
                true,
                true,
            ),
        )
        connectivity.activeNetwork?.also { network ->
            setNetworkCapabilities(network, capabilities(validated))
        }
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

    private data class MonitorFixture(
        val connectivity: ConnectivityManager,
        val shadow: ShadowConnectivityManager,
        val monitor: NetworkReturnMonitor,
        val returns: () -> Int,
    )
}
