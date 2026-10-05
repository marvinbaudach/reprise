package io.github.marvinbaudach.reprise.widget

import android.util.Log
import io.github.marvinbaudach.reprise.library.TrackMetadata
import java.util.concurrent.Executor
import java.util.concurrent.RejectedExecutionException
import uniffi.reprise_android_ffi.AndroidPlaybackSnapshot

private const val TAG = "RepriseWidget"

/**
 * Turns playback changes into widget redraws.
 *
 * Every playback snapshot passes through here, and most of them — position
 * ticks, volume, shuffle — change nothing the widget shows. Only a different
 * track or a play/pause flip costs a metadata read and a redraw.
 *
 * The reads block (they go through the library), so [executor] must not be the
 * main thread; [refresh] asks the widget to draw again.
 */
internal class WidgetPublisher(
    private val executor: Executor,
    private val store: WidgetStateStore,
    private val metadata: (trackUri: String) -> TrackMetadata?,
    private val artworkPath: (trackUri: String) -> String?,
    private val refresh: () -> Unit,
) {
    private var published: WidgetStateKey? = null
    private var current: WidgetNowPlaying? = null

    fun onSnapshot(snapshot: AndroidPlaybackSnapshot?) {
        val key = snapshot.widgetKey()
        synchronized(this) {
            if (key == published) return
            published = key
        }
        try {
            executor.execute { publish(snapshot) }
        } catch (error: RejectedExecutionException) {
            // The service is shutting down; its last snapshot has nobody to draw for.
            Log.d(TAG, "The widget publisher is shut down", error)
        }
    }

    private fun publish(snapshot: AndroidPlaybackSnapshot?) {
        try {
            val previous = synchronized(this) { current } ?: store.load()
            val next = widgetNowPlaying(snapshot, previous, metadata, artworkPath)
            synchronized(this) { current = next }
            if (next == previous) return
            store.save(next)
            refresh()
        } catch (error: Exception) {
            // A widget that cannot update is a stale widget, not a reason to
            // disturb playback.
            Log.w(TAG, "Could not update the widget", error)
        } catch (error: LinkageError) {
            Log.w(TAG, "Could not update the widget", error)
        }
    }
}
