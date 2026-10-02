package io.github.marvinbaudach.reprise

import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.os.Handler
import android.os.Looper
import android.util.Log

internal const val COVER_RETRY_TAG = "RepriseCoverRetry"

/** Reports only validated offline-to-online transitions after a baseline. */
internal class NetworkReturnDetector {
    private var online: Boolean? = null

    @Synchronized
    fun observe(online: Boolean, onReturn: () -> Unit = {}): Boolean {
        val returned = this.online == false && online
        this.online = online
        if (returned) onReturn()
        return returned
    }
}

/** Adapts Android's physical-network callbacks to the retained pure detector. */
internal class NetworkReturnMonitor(
    private val connectivity: ConnectivityManager,
    private val detector: NetworkReturnDetector,
    private val onNetworkReturned: () -> Unit,
    private val postToMain: (() -> Unit) -> Unit = { work ->
        Handler(Looper.getMainLooper()).post(work)
    },
    private val logReturn: (String, Int) -> Unit = { transports, count ->
        Log.i(
            COVER_RETRY_TAG,
            "Network returned: validatedNonVpn=$count, transports=$transports",
        )
    },
) {
    private var registered = false
    private val validatedNetworks = mutableMapOf<Network, NetworkCapabilities>()
    private val request = NetworkRequest.Builder()
        .addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
        .addCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN)
        .build()

    private val callback = object : ConnectivityManager.NetworkCallback() {
        override fun onCapabilitiesChanged(network: Network, capabilities: NetworkCapabilities) {
            update(network, capabilities)
        }

        override fun onLost(network: Network) {
            update(network, null)
        }
    }

    fun start() {
        if (registered) return
        observeCurrentNetworks()
        try {
            connectivity.registerNetworkCallback(request, callback)
            registered = true
        } catch (error: RuntimeException) {
            Log.w(COVER_RETRY_TAG, "Could not monitor network returns", error)
        }
    }

    fun stop() {
        if (!registered) return
        connectivity.unregisterNetworkCallback(callback)
        registered = false
    }

    private fun observeCurrentNetworks() {
        val current = connectivity.allNetworks.mapNotNull { network ->
            connectivity.getNetworkCapabilities(network)
                ?.takeIf { capabilities -> capabilities.isValidatedPhysicalNetwork() }
                ?.let { capabilities -> network to capabilities }
        }.toMap()
        val returnedNetworks = synchronized(validatedNetworks) {
            validatedNetworks.clear()
            validatedNetworks.putAll(current)
            if (detector.observe(validatedNetworks.isNotEmpty())) {
                validatedNetworks.values.toList()
            } else {
                null
            }
        }
        returnedNetworks?.let(::reportReturn)
    }

    private fun update(network: Network, capabilities: NetworkCapabilities?) {
        val returnedNetworks = synchronized(validatedNetworks) {
            if (capabilities.isValidatedPhysicalNetwork()) {
                validatedNetworks[network] = requireNotNull(capabilities)
            } else {
                validatedNetworks.remove(network)
            }
            if (detector.observe(validatedNetworks.isNotEmpty())) {
                validatedNetworks.values.toList()
            } else {
                null
            }
        }
        returnedNetworks?.let(::reportReturn)
    }

    private fun reportReturn(networks: List<NetworkCapabilities>) {
        val transports = networks
            .flatMap(NetworkCapabilities::transportNames)
            .distinct()
            .sorted()
            .joinToString().ifEmpty { "other" }
        logReturn(transports, networks.size)
        postToMain(onNetworkReturned)
    }
}

private fun NetworkCapabilities?.isValidatedPhysicalNetwork(): Boolean =
    this != null &&
        hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) &&
        hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED) &&
        hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN)

private fun NetworkCapabilities.transportNames(): List<String> = buildList {
    if (hasTransport(NetworkCapabilities.TRANSPORT_WIFI)) add("wifi")
    if (hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR)) add("cellular")
    if (hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET)) add("ethernet")
}
