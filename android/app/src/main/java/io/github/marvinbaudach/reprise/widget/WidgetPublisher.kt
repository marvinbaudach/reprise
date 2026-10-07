package io.github.marvinbaudach.reprise.widget

import android.os.SystemClock
import android.util.Log
import io.github.marvinbaudach.reprise.library.PlaybackKey
import io.github.marvinbaudach.reprise.library.TrackMetadata
import java.util.concurrent.Executor
import java.util.concurrent.RejectedExecutionException
import uniffi.reprise_android_ffi.AndroidPlaybackSnapshot

private const val TAG = "RepriseWidget"

/** How long a failed update waits before the next snapshot may try it again. */
internal const val RETRY_DELAY_MS = 5_000L

/**
 * Turns playback changes into widget redraws.
 *
 * Every playback snapshot passes through here, and most of them — position
 * ticks, volume, shuffle — change nothing the widget shows. Only a different
 * track or a play/pause flip costs a metadata read and a redraw.
 *
 * A change counts as shown only once it was shown: an update that failed (the
 * library did not answer, the store could not write) is tried again by a later
 * snapshot, after [RETRY_DELAY_MS] so a library that keeps failing is not asked
 * on every position tick.
 *
 * The reads block (they go through the library), so [executor] must not be the
 * main thread; [refresh] asks the widget to draw again.
 */
internal class WidgetPublisher(
    private val executor: Executor,
    private val store: WidgetStateStore,
    private val metadata: (PlaybackKey) -> TrackMetadata?,
    private val artworkPath: (trackUri: String) -> String?,
    private val refresh: () -> Unit,
    private val now: () -> Long = SystemClock::elapsedRealtime,
) {
    private var published: WidgetStateKey? = null
    private var pending: WidgetStateKey? = null
    private var retryNotBefore = 0L
    private var latest: AndroidPlaybackSnapshot? = null
    private var current: WidgetNowPlaying? = null

    fun onSnapshot(snapshot: AndroidPlaybackSnapshot?) {
        val key = snapshot.widgetKey()
        synchronized(this) {
            latest = snapshot
            if (key == published || key == pending || now() < retryNotBefore) return
            pending = key
        }
        submit(snapshot, key)
    }

    /**
     * A cover landed for the track on show. The widget is drawn again if it had
     * none, since nothing else would tell it: the playback state did not change.
     */
    fun onArtworkAvailable() {
        val snapshot = synchronized(this) { latest } ?: return
        submit(snapshot, snapshot.widgetKey())
    }

    private fun submit(snapshot: AndroidPlaybackSnapshot?, key: WidgetStateKey) {
        try {
            executor.execute { publish(snapshot, key) }
        } catch (error: RejectedExecutionException) {
            // The service is shutting down; its last snapshot has nobody to draw for.
            Log.d(TAG, "The widget publisher is shut down", error)
            synchronized(this) { if (pending == key) pending = null }
        }
    }

    private fun publish(snapshot: AndroidPlaybackSnapshot?, key: WidgetStateKey) {
        val succeeded = try {
            val previous = synchronized(this) { current } ?: store.load()
            val next = widgetNowPlaying(snapshot, previous, metadata, artworkPath)
            synchronized(this) { current = next }
            if (next != previous) {
                store.save(next)
                refresh()
            }
            true
        } catch (error: Exception) {
            // A widget that cannot update is a stale widget, not a reason to
            // disturb playback.
            Log.w(TAG, "Could not update the widget", error)
            false
        } catch (error: LinkageError) {
            Log.w(TAG, "Could not update the widget", error)
            false
        }
        synchronized(this) {
            if (pending == key) pending = null
            if (succeeded) {
                published = key
                retryNotBefore = 0L
            } else {
                retryNotBefore = now() + RETRY_DELAY_MS
            }
        }
    }
}
