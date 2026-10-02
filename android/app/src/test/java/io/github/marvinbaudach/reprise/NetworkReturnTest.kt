package io.github.marvinbaudach.reprise

import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.net.NetworkInfo
import android.net.NetworkRequest
import android.os.Looper
import android.util.Log
import androidx.test.core.app.ApplicationProvider
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.shadows.ShadowConnectivityManager
import org.robolectric.shadows.ShadowLog
import org.robolectric.shadows.ShadowNetwork
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
    fun a_cold_start_without_a_physical_network_reports_the_first_validated_network() {
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
    fun switching_between_validated_physical_networks_is_not_a_return() {
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

    @Test
    fun net_7b_a_validated_vpn_does_not_hide_the_physical_network_return() {
        val fixture = monitorFixture(activeValidated = null)
        val vpn = fixture.addNetwork(
            id = 40,
            type = ConnectivityManager.TYPE_VPN,
            capabilities = capabilities(
                validated = true,
                transport = NetworkCapabilities.TRANSPORT_VPN,
                notVpn = false,
            ),
        )
        fixture.shadow.setActiveNetworkInfo(networkInfo(ConnectivityManager.TYPE_VPN))
        fixture.shadow.setNetworkCapabilities(
            requireNotNull(fixture.connectivity.activeNetwork),
            fixture.connectivity.getNetworkCapabilities(vpn)!!,
        )
        val wifi = fixture.addNetwork(
            id = 41,
            type = ConnectivityManager.TYPE_WIFI,
            capabilities = capabilities(validated = true),
        )

        fixture.monitor.start()
        val callback = fixture.shadow.networkCallbacks.single()
        callback.onLost(wifi)
        callback.onCapabilitiesChanged(
            ShadowNetwork.newInstance(42),
            capabilities(validated = true),
        )

        assertEquals(1, fixture.returns())
        fixture.monitor.stop()
    }

    @Test
    fun validated_wifi_while_cellular_is_validated_is_not_a_return() {
        val fixture = monitorFixture(activeValidated = null)
        fixture.addNetwork(
            id = 43,
            type = ConnectivityManager.TYPE_MOBILE,
            capabilities = capabilities(
                validated = true,
                transport = NetworkCapabilities.TRANSPORT_CELLULAR,
            ),
        )

        fixture.monitor.start()
        fixture.shadow.networkCallbacks.single().onCapabilitiesChanged(
            ShadowNetwork.newInstance(44),
            capabilities(validated = true),
        )

        assertEquals(0, fixture.returns())
        fixture.monitor.stop()
    }

    @Test
    fun an_available_network_returns_only_when_it_becomes_validated() {
        val fixture = monitorFixture(activeValidated = null)
        val wifi = ShadowNetwork.newInstance(45)

        fixture.monitor.start()
        val callback = fixture.shadow.networkCallbacks.single()
        callback.onAvailable(wifi)
        assertEquals(0, fixture.returns())
        callback.onCapabilitiesChanged(wifi, capabilities(validated = true))

        assertEquals(1, fixture.returns())
        fixture.monitor.stop()
    }

    @Test
    fun a_cold_start_with_only_a_validated_vpn_has_an_offline_baseline() {
        val fixture = monitorFixture(activeValidated = null)
        fixture.addNetwork(
            id = 46,
            type = ConnectivityManager.TYPE_VPN,
            capabilities = capabilities(
                validated = true,
                transport = NetworkCapabilities.TRANSPORT_VPN,
                notVpn = false,
            ),
        )

        fixture.monitor.start()
        fixture.shadow.networkCallbacks.single().onCapabilitiesChanged(
            ShadowNetwork.newInstance(47),
            capabilities(validated = true),
        )

        assertEquals(1, fixture.returns())
        fixture.monitor.stop()
    }

    @Test
    fun a_view_model_retains_the_detector_across_monitor_recreation() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val connectivity = context.getSystemService(ConnectivityManager::class.java)
        val shadow = shadowOf(connectivity)
        shadow.clearAllNetworks()
        shadow.setActiveNetworkInfo(null)
        val viewModel = MobileSurfaceViewModel()
        var returns = 0

        viewModel.startNetworkReturnMonitor(context) { returns += 1 }
        viewModel.stopNetworkReturnMonitor()
        val wifi = ShadowNetwork.newInstance(48)
        shadow.addNetwork(wifi, networkInfo(ConnectivityManager.TYPE_WIFI))
        shadow.setNetworkCapabilities(wifi, capabilities(validated = true))
        viewModel.startNetworkReturnMonitor(context) { returns += 1 }
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(1, returns)
        viewModel.stopNetworkReturnMonitor()
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

    private fun capabilities(
        validated: Boolean,
        transport: Int = NetworkCapabilities.TRANSPORT_WIFI,
        notVpn: Boolean = true,
    ): NetworkCapabilities {
        val capabilities = ShadowNetworkCapabilities.newInstance()
        shadowOf(capabilities).apply {
            addTransportType(transport)
            addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
            if (notVpn) {
                addCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN)
            } else {
                removeCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN)
            }
            if (validated) addCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
        }
        return capabilities
    }

    private fun networkInfo(type: Int): NetworkInfo = ShadowNetworkInfo.newInstance(
        NetworkInfo.DetailedState.CONNECTED,
        type,
        0,
        true,
        true,
    )

    private data class MonitorFixture(
        val connectivity: ConnectivityManager,
        val shadow: ShadowConnectivityManager,
        val monitor: NetworkReturnMonitor,
        val returns: () -> Int,
    ) {
        fun addNetwork(
            id: Int,
            type: Int,
            capabilities: NetworkCapabilities,
        ): android.net.Network = ShadowNetwork.newInstance(id).also { network ->
            shadow.addNetwork(network, networkInfo(type))
            shadow.setNetworkCapabilities(network, capabilities)
        }

        private fun networkInfo(type: Int): NetworkInfo = ShadowNetworkInfo.newInstance(
            NetworkInfo.DetailedState.CONNECTED,
            type,
            0,
            true,
            true,
        )
    }
}

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], shadows = [FailingRegistrationConnectivityManagerShadow::class])
class NetworkReturnRegistrationTest {
    @Test
    fun registration_failure_is_logged_and_the_next_start_retries() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val connectivity = context.getSystemService(ConnectivityManager::class.java)
        val shadow = shadowOf(connectivity) as FailingRegistrationConnectivityManagerShadow
        shadow.clearAllNetworks()
        shadow.setActiveNetworkInfo(null)
        val monitor = NetworkReturnMonitor(
            connectivity = connectivity,
            detector = NetworkReturnDetector(),
            onNetworkReturned = {},
            postToMain = { work -> work() },
        )

        ShadowLog.clear()
        monitor.start()

        assertEquals(1, shadow.registrationAttempts)
        assertTrue(shadow.networkCallbacks.isEmpty())
        assertTrue(
            ShadowLog.getLogsForTag(COVER_RETRY_TAG).any { item ->
                item.type == Log.WARN && item.msg.contains("Could not monitor network returns")
            },
        )

        monitor.start()

        assertEquals(2, shadow.registrationAttempts)
        assertEquals(1, shadow.networkCallbacks.size)
        monitor.stop()
    }
}

@Implements(ConnectivityManager::class)
class FailingRegistrationConnectivityManagerShadow : ShadowConnectivityManager() {
    var registrationAttempts = 0

    @Implementation
    override fun registerNetworkCallback(
        request: NetworkRequest,
        callback: ConnectivityManager.NetworkCallback,
    ) {
        registrationAttempts += 1
        if (registrationAttempts == 1) error("registration failed")
        super.registerNetworkCallback(request, callback)
    }
}
