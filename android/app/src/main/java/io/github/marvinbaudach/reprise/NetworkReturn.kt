package io.github.marvinbaudach.reprise

import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.os.Handler
import android.os.Looper
import android.util.Log

internal const val COVER_RETRY_TAG = "RepriseCoverRetry"

/** Reports only validated offline-to-online transitions after a baseline. */
internal class NetworkReturnDetector {
    private var online: Boolean? = null

    fun observe(online: Boolean, onReturn: () -> Unit = {}): Boolean {
        val returned = this.online == false && online
        this.online = online
        if (returned) onReturn()
        return returned
    }
}

/** Adapts Android's default-network callbacks to the retained pure detector. */
internal class NetworkReturnMonitor(
    private val connectivity: ConnectivityManager,
    private val detector: NetworkReturnDetector,
    private val onNetworkReturned: () -> Unit,
    private val postToMain: (() -> Unit) -> Unit = { work ->
        Handler(Looper.getMainLooper()).post(work)
    },
    private val logReturn: (String, Boolean) -> Unit = { transport, validated ->
        Log.i(COVER_RETRY_TAG, "Default network returned: $transport, validated=$validated")
    },
) {
    private var registered = false

    private val callback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) {
            connectivity.getNetworkCapabilities(network)?.let(::observe)
        }

        override fun onCapabilitiesChanged(network: Network, capabilities: NetworkCapabilities) {
            observe(capabilities)
        }

        override fun onLost(network: Network) {
            observeCurrentNetwork()
        }
    }

    fun start() {
        if (registered) return
        observeCurrentNetwork()
        connectivity.registerDefaultNetworkCallback(callback)
        registered = true
    }

    fun stop() {
        if (!registered) return
        connectivity.unregisterNetworkCallback(callback)
        registered = false
    }

    private fun observeCurrentNetwork() {
        val capabilities = connectivity.activeNetwork?.let(connectivity::getNetworkCapabilities)
        if (capabilities == null) {
            detector.observe(online = false)
        } else {
            observe(capabilities)
        }
    }

    private fun observe(capabilities: NetworkCapabilities) {
        val validated = capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
        val online = validated &&
            capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
        detector.observe(online) {
            val transport = capabilities.transportName()
            logReturn(transport, validated)
            postToMain(onNetworkReturned)
        }
    }
}

private fun NetworkCapabilities.transportName(): String = when {
    hasTransport(NetworkCapabilities.TRANSPORT_VPN) -> "vpn"
    hasTransport(NetworkCapabilities.TRANSPORT_WIFI) -> "wifi"
    hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) -> "cellular"
    hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) -> "ethernet"
    else -> "other"
}
