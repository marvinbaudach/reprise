package io.github.marvinbaudach.reprise

/** Reports only validated offline-to-online transitions after a baseline. */
internal class NetworkReturnDetector {
    private var online: Boolean? = null

    fun observe(online: Boolean): Boolean {
        val returned = this.online == false && online
        this.online = online
        return returned
    }
}
