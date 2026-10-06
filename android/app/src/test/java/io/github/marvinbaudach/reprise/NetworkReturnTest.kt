package io.github.marvinbaudach.reprise

import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.net.NetworkInfo
import android.net.NetworkRequest
import android.os.Looper
import android.util.Log
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ApplicationProvider
import java.time.Duration
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
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
    fun a_return_callback_in_flight_during_stop_still_delivers_the_immediate_bump() {
        val posted = mutableListOf<() -> Unit>()
        val scheduler = FakeNetworkReturnScheduler()
        val fixture = monitorFixture(
            activeValidated = null,
            scheduler = scheduler,
            postToMain = posted::add,
        )

        fixture.monitor.start()
        posted.forEach { work -> work() }
        posted.clear()
        fixture.returnNetwork(id = 49)
        assertEquals(0, fixture.returns())

        fixture.monitor.stop()
        posted.single().invoke()

        assertEquals(1, fixture.returns())
        assertEquals(emptyList<Long>(), scheduler.pendingDelays())
    }

    @Test
    fun net_7b_a_return_schedules_three_follow_ups() {
        val scheduler = FakeNetworkReturnScheduler()
        val followUpLogs = mutableListOf<Int>()
        val fixture = monitorFixture(
            activeValidated = null,
            scheduler = scheduler,
            logFollowUp = followUpLogs::add,
        )

        fixture.monitor.start()
        fixture.returnNetwork(id = 50)

        assertEquals(1, fixture.returns())
        assertEquals(listOf(3_000L, 10_000L, 30_000L), scheduler.pendingDelays())

        scheduler.advanceBy(3_000L)
        assertEquals(2, fixture.returns())
        scheduler.advanceBy(7_000L)
        assertEquals(3, fixture.returns())
        scheduler.advanceBy(20_000L)
        assertEquals(4, fixture.returns())
        assertEquals(listOf(1, 2, 3), followUpLogs)
        assertEquals(emptyList<Long>(), scheduler.pendingDelays())
        fixture.monitor.stop()
    }

    @Test
    fun losing_the_network_cancels_the_remaining_follow_ups() {
        val scheduler = FakeNetworkReturnScheduler()
        val fixture = monitorFixture(activeValidated = null, scheduler = scheduler)

        fixture.monitor.start()
        val network = fixture.returnNetwork(id = 51)
        scheduler.advanceBy(3_000L)
        fixture.shadow.networkCallbacks.single().onLost(network)
        scheduler.advanceBy(27_000L)

        assertEquals(2, fixture.returns())
        assertEquals(emptyList<Long>(), scheduler.pendingDelays())
        fixture.monitor.stop()
    }

    @Test
    fun a_second_return_replaces_the_first_follow_up_schedule() {
        val scheduler = FakeNetworkReturnScheduler()
        val fixture = monitorFixture(activeValidated = null, scheduler = scheduler)

        fixture.monitor.start()
        val first = fixture.returnNetwork(id = 52)
        scheduler.advanceBy(5_000L)
        fixture.shadow.networkCallbacks.single().onLost(first)
        fixture.returnNetwork(id = 53)

        assertEquals(3, fixture.returns())
        assertEquals(listOf(3_000L, 10_000L, 30_000L), scheduler.pendingDelays())
        scheduler.advanceBy(30_000L)
        assertEquals(6, fixture.returns())
        fixture.monitor.stop()
    }

    @Test
    fun stop_cancels_follow_ups_and_the_next_start_can_schedule_a_fresh_set() {
        val scheduler = FakeNetworkReturnScheduler()
        val fixture = monitorFixture(activeValidated = null, scheduler = scheduler)

        fixture.monitor.start()
        val first = fixture.returnNetwork(id = 54)
        fixture.monitor.stop()
        assertEquals(emptyList<Long>(), scheduler.pendingDelays())

        fixture.shadow.removeNetwork(first)
        fixture.monitor.start()
        fixture.monitor.stop()
        fixture.addNetwork(
            id = 55,
            type = ConnectivityManager.TYPE_WIFI,
            capabilities = capabilities(validated = true),
        )
        fixture.monitor.start()

        assertEquals(2, fixture.returns())
        assertEquals(listOf(3_000L, 10_000L, 30_000L), scheduler.pendingDelays())
        fixture.monitor.stop()
    }

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

    @Test
    fun a_view_model_delivers_an_in_flight_return_after_a_real_stop_and_restart() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val connectivity = context.getSystemService(ConnectivityManager::class.java)
        val shadow = shadowOf(connectivity)
        shadow.clearAllNetworks()
        shadow.setActiveNetworkInfo(null)
        val viewModel = MobileSurfaceViewModel()
        var returns = 0

        viewModel.startNetworkReturnMonitor(context) { returns += 1 }
        val wifi = ShadowNetwork.newInstance(49)
        val wifiCapabilities = capabilities(validated = true)
        shadow.addNetwork(wifi, networkInfo(ConnectivityManager.TYPE_WIFI))
        shadow.setNetworkCapabilities(wifi, wifiCapabilities)
        shadow.networkCallbacks.single().onCapabilitiesChanged(wifi, wifiCapabilities)
        viewModel.stopNetworkReturnMonitor()
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(0, returns)

        viewModel.startNetworkReturnMonitor(context) { returns += 1 }
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(1, returns)
        viewModel.stopNetworkReturnMonitor()
    }

    private fun monitorFixture(
        activeValidated: Boolean?,
        scheduler: NetworkReturnScheduler = FakeNetworkReturnScheduler(),
        logFollowUp: (Int) -> Unit = {},
        postToMain: ((() -> Unit) -> Unit) = { work -> work() },
    ): MonitorFixture {
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
            postToMain = postToMain,
            scheduler = scheduler,
            logFollowUp = logFollowUp,
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
        fun returnNetwork(id: Int): android.net.Network = ShadowNetwork.newInstance(id).also { network ->
            shadow.networkCallbacks.single().onCapabilitiesChanged(
                network,
                capabilities(validated = true),
            )
        }

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
    }
}

private class FakeNetworkReturnScheduler : NetworkReturnScheduler {
    private data class Scheduled(
        val dueAtMs: Long,
        val order: Long,
        val work: Runnable,
    )

    private var nowMs = 0L
    private var nextOrder = 0L
    private val scheduled = mutableListOf<Scheduled>()

    override fun postDelayed(work: Runnable, delayMs: Long) {
        scheduled += Scheduled(nowMs + delayMs, nextOrder++, work)
    }

    override fun cancel(work: Runnable) {
        scheduled.removeAll { it.work === work }
    }

    fun pendingDelays(): List<Long> = scheduled
        .sortedWith(compareBy(Scheduled::dueAtMs, Scheduled::order))
        .map { it.dueAtMs - nowMs }

    fun advanceBy(delayMs: Long) {
        val targetMs = nowMs + delayMs
        while (true) {
            val next = scheduled
                .filter { it.dueAtMs <= targetMs }
                .minWithOrNull(compareBy(Scheduled::dueAtMs, Scheduled::order))
                ?: break
            scheduled.remove(next)
            nowMs = next.dueAtMs
            next.work.run()
        }
        nowMs = targetMs
    }
}

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], shadows = [FailingRegistrationConnectivityManagerShadow::class])
class NetworkReturnRegistrationTest {
    @Test
    fun a_return_observed_before_registration_failure_still_delivers_the_immediate_bump() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val connectivity = context.getSystemService(ConnectivityManager::class.java)
        val shadow = shadowOf(connectivity) as FailingRegistrationConnectivityManagerShadow
        shadow.clearAllNetworks()
        shadow.setActiveNetworkInfo(null)
        val network = ShadowNetwork.newInstance(70)
        val capabilities = ShadowNetworkCapabilities.newInstance().also { networkCapabilities ->
            shadowOf(networkCapabilities).apply {
                addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
                addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
                addCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN)
                addCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
            }
        }
        shadow.addNetwork(network, ShadowNetworkInfo.newInstance(
            NetworkInfo.DetailedState.CONNECTED,
            ConnectivityManager.TYPE_WIFI,
            0,
            true,
            true,
        ))
        shadow.setNetworkCapabilities(network, capabilities)
        val detector = NetworkReturnDetector().also { it.observe(online = false) }
        val scheduler = FakeNetworkReturnScheduler()
        val posted = mutableListOf<() -> Unit>()
        var returns = 0
        val monitor = NetworkReturnMonitor(
            connectivity = connectivity,
            detector = detector,
            onNetworkReturned = { returns += 1 },
            postToMain = posted::add,
            scheduler = scheduler,
        )

        monitor.start()
        posted.single().invoke()

        assertEquals(1, returns)
        assertEquals(emptyList<Long>(), scheduler.pendingDelays())
    }

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

@RunWith(RobolectricTestRunner::class)
@Config(
    sdk = [36],
    application = NetworkReturnRotationTestApplication::class,
)
class NetworkReturnRotationTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    @After
    fun releaseTheService() {
        application.release()
    }

    @Test
    fun net_7b_rotation_keeps_the_pending_follow_up_schedule() {
        val connectivity = RuntimeEnvironment.getApplication()
            .getSystemService(ConnectivityManager::class.java)
        val shadow = shadowOf(connectivity)
        shadow.clearAllNetworks()
        shadow.setActiveNetworkInfo(null)
        compose.waitForIdle()
        val beforeRotationArtwork = application.composedArtwork.single()
        val beforeRotation = ViewModelProvider(compose.activity)[MobileSurfaceViewModel::class.java]
        beforeRotation.startNetworkReturnMonitor(
            compose.activity,
            beforeRotationArtwork::networkReturned,
        )

        shadow.networkCallbacks.single().onCapabilitiesChanged(
            ShadowNetwork.newInstance(71),
            ShadowNetworkCapabilities.newInstance().also { capabilities ->
                shadowOf(capabilities).apply {
                    addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
                    addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
                    addCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN)
                    addCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
                }
            },
        )
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(1L, beforeRotationArtwork.networkReturnRevision)

        compose.activityRule.scenario.recreate()
        compose.waitForIdle()
        val afterRotation = ViewModelProvider(compose.activity)[MobileSurfaceViewModel::class.java]
        val afterRotationArtwork = application.composedArtwork.last()
        assertTrue(beforeRotation === afterRotation)
        assertFalse(beforeRotationArtwork === afterRotationArtwork)
        afterRotation.startNetworkReturnMonitor(
            compose.activity,
            afterRotationArtwork::networkReturned,
        )
        shadowOf(Looper.getMainLooper()).idleFor(
            Duration.ofMillis(FIRST_NETWORK_RETURN_FOLLOW_UP_DELAY_MS),
        )

        assertEquals(1L, beforeRotationArtwork.networkReturnRevision)
        assertEquals(1L, afterRotationArtwork.networkReturnRevision)
        afterRotation.stopNetworkReturnMonitor()
    }

    private val application: NetworkReturnRotationTestApplication
        get() = RuntimeEnvironment.getApplication() as NetworkReturnRotationTestApplication
}

internal class NetworkReturnRotationTestApplication : ConfigurationTestApplication() {
    val composedArtwork = mutableListOf<TrackArtwork>()
    private val createdArtwork = mutableListOf<TrackArtwork>()

    override fun mainActivitySurface(): MainActivitySurfaceDependencies {
        val artwork = TrackArtwork(resolve = { _, _ -> null })
        createdArtwork += artwork
        return super.mainActivitySurface().copy(
            artwork = {
                if (composedArtwork.lastOrNull() !== artwork) composedArtwork += artwork
                artwork
            },
        )
    }

    fun release() {
        createdArtwork.forEach(TrackArtwork::shutdown)
        releaseService()
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
