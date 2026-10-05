package io.github.marvinbaudach.reprise.library

import android.net.Uri
import android.util.Log
import java.util.concurrent.Executor

private const val TAG = "RepriseArtwork"

/**
 * Fetches the cover of whichever track is playing and hands it to the player
 * once it is there.
 *
 * A cover comes from the music folder through the document provider, so it
 * cannot be had on the playback path without delaying the first note. The item
 * therefore starts without one and gains it here, a moment later.
 *
 * [resolve] blocks and runs on [executor]; [attach] runs wherever the caller
 * wants it to, and is told which uri the cover belongs to so a late answer
 * lands on the right item or on none.
 */
internal class CurrentTrackArtwork(
    private val executor: Executor,
    private val resolve: (trackUri: String) -> Uri?,
    private val attach: (trackUri: String, artwork: Uri) -> Unit,
) {
    private var requested: String? = null

    /** Call on every playback change; only a different track starts a fetch. */
    @Synchronized
    fun onCurrentTrack(trackUri: String?) {
        if (trackUri == null || trackUri == requested) return
        requested = trackUri
        executor.execute {
            val artwork = try {
                resolve(trackUri)
            } catch (error: Exception) {
                Log.w(TAG, "Could not load the cover of the playing track", error)
                null
            }
            if (artwork != null) attach(trackUri, artwork)
        }
    }
}
