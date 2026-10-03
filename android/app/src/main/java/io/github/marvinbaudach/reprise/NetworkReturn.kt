package io.github.marvinbaudach.reprise

import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.os.Handler
import android.os.Looper
import android.util.Log

internal const val COVER_RETRY_TAG = "RepriseCoverRetry"
internal const val FIRST_NETWORK_RETURN_FOLLOW_UP_DELAY_MS = 3_000L
internal const val SECOND_NETWORK_RETURN_FOLLOW_UP_DELAY_MS = 10_000L
internal const val THIRD_NETWORK_RETURN_FOLLOW_UP_DELAY_MS = 30_000L

private val NETWORK_RETURN_FOLLOW_UP_DELAYS_MS = listOf(
    FIRST_NETWORK_RETURN_FOLLOW_UP_DELAY_MS,
    SECOND_NETWORK_RETURN_FOLLOW_UP_DELAY_MS,
    THIRD_NETWORK_RETURN_FOLLOW_UP_DELAY_MS,
)

internal interface NetworkReturnScheduler {
    fun postDelayed(work: Runnable, delayMs: Long)

    fun cancel(work: Runnable)
}

private class HandlerNetworkReturnScheduler(
    private val handler: Handler = Handler(Looper.getMainLooper()),
) : NetworkReturnScheduler {
    override fun postDelayed(work: Runnable, delayMs: Long) {
        handler.postDelayed(work, delayMs)
    }

    override fun cancel(work: Runnable) {
        handler.removeCallbacks(work)
    }
}

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
    private val scheduler: NetworkReturnScheduler = HandlerNetworkReturnScheduler(),
    private val logFollowUp: (Int) -> Unit = { followUp ->
        Log.i(COVER_RETRY_TAG, "Network return follow-up $followUp/3")
    },
) {
    @Volatile
    private var started = false
    private var registered = false
    private var followUpGeneration = 0L
    private val pendingFollowUps = mutableListOf<Runnable>()
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
        if (started) return
        started = true
        observeCurrentNetworks()
        try {
            connectivity.registerNetworkCallback(request, callback)
            registered = true
        } catch (error: RuntimeException) {
            started = false
            cancelFollowUps()
            Log.w(COVER_RETRY_TAG, "Could not monitor network returns", error)
        }
    }

    fun stop() {
        started = false
        cancelFollowUps()
        if (registered) {
            connectivity.unregisterNetworkCallback(callback)
            registered = false
        }
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
        deliverObservation(returnedNetworks, current.isEmpty())
    }

    private fun update(network: Network, capabilities: NetworkCapabilities?) {
        val (returnedNetworks, offline) = synchronized(validatedNetworks) {
            if (capabilities.isValidatedPhysicalNetwork()) {
                validatedNetworks[network] = requireNotNull(capabilities)
            } else {
                validatedNetworks.remove(network)
            }
            val networks = if (detector.observe(validatedNetworks.isNotEmpty())) {
                validatedNetworks.values.toList()
            } else {
                null
            }
            networks to validatedNetworks.isEmpty()
        }
        deliverObservation(returnedNetworks, offline)
    }

    private fun deliverObservation(
        returnedNetworks: List<NetworkCapabilities>?,
        offline: Boolean,
    ) {
        when {
            returnedNetworks != null -> reportReturn(returnedNetworks)
            offline -> postToMain {
                if (started) cancelFollowUps()
            }
        }
    }

    private fun reportReturn(networks: List<NetworkCapabilities>) {
        val transports = networks
            .flatMap(NetworkCapabilities::transportNames)
            .distinct()
            .sorted()
            .joinToString().ifEmpty { "other" }
        logReturn(transports, networks.size)
        postToMain {
            if (!started) return@postToMain
            cancelFollowUps()
            onNetworkReturned()
            scheduleFollowUps()
        }
    }

    private fun scheduleFollowUps() {
        val generation = followUpGeneration
        NETWORK_RETURN_FOLLOW_UP_DELAYS_MS.forEachIndexed { index, delayMs ->
            lateinit var work: Runnable
            work = Runnable {
                pendingFollowUps.remove(work)
                if (!started || generation != followUpGeneration) return@Runnable
                logFollowUp(index + 1)
                onNetworkReturned()
            }
            pendingFollowUps += work
            scheduler.postDelayed(work, delayMs)
        }
    }

    private fun cancelFollowUps() {
        followUpGeneration += 1
        val cancelled = pendingFollowUps.toList()
        pendingFollowUps.clear()
        cancelled.forEach(scheduler::cancel)
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
