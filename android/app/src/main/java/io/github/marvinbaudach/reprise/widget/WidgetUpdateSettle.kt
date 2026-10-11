package io.github.marvinbaudach.reprise.widget

import android.os.Handler
import uniffi.reprise_android_ffi.AndroidPlaybackSnapshot

/**
 * How long a changed [WidgetStateKey] must stay unchanged before the widget is
 * drawn for it. Longer than the song swipe's settle (`NOW_PLAYING_SETTLE_MS`,
 * 300 ms) plus the fog crossfade (`FOG_CROSSFADE_MS`, 500 ms), so the redraw
 * lands after both animations rather than inside them.
 *
 * Measured with simpleperf on a release build: Glance composes the widget on the
 * main thread and copies every cover into ashmem there
 * (`RemoteViews.setImageViewBitmap` -> `Bitmap.asShared`), 24-124 ms in ~16
 * slices of 7-20 ms per track change, starting ~0.2 s after the swipe. That
 * dropped frames and once froze the swipe for 110 ms.
 */
internal const val WIDGET_UPDATE_SETTLE_MS = 900L

/**
 * Holds back a widget update until its key has been quiet for [settleMs].
 *
 * A key change (another track, play/pause) starts the wait; a further change
 * restarts it, so a run of quick skips draws the widget once, for the last
 * track. Snapshots that leave the key as it is neither restart nor extend the
 * wait: while one is pending they only refresh the snapshot that will be
 * delivered, and once nothing is pending they pass straight through, because
 * [WidgetPublisher] relies on seeing every snapshot to retry a failed update.
 *
 * The last change is always delivered: a wait is replaced, never dropped. Only
 * [cancel] drops it, for a service that is going away.
 */
internal class WidgetUpdateSettle(
    private val handler: Handler,
    private val settleMs: Long = WIDGET_UPDATE_SETTLE_MS,
    private val deliver: (AndroidPlaybackSnapshot?) -> Unit,
) {
    private var seen = false
    private var lastKey: WidgetStateKey? = null
    private var waiting = false
    private var latest: AndroidPlaybackSnapshot? = null

    private val deliverLatest = Runnable {
        val snapshot = synchronized(this) {
            waiting = false
            latest
        }
        deliver(snapshot)
    }

    fun onSnapshot(snapshot: AndroidPlaybackSnapshot?) {
        val key = snapshot.widgetKey()
        val passThrough = synchronized(this) {
            latest = snapshot
            if (!seen || key != lastKey) {
                seen = true
                lastKey = key
                waiting = true
                handler.removeCallbacks(deliverLatest)
                handler.postDelayed(deliverLatest, settleMs)
                false
            } else {
                !waiting
            }
        }
        if (passThrough) deliver(snapshot)
    }

    /** Drops the update that is waiting, if any. */
    fun cancel() {
        synchronized(this) {
            waiting = false
            handler.removeCallbacks(deliverLatest)
        }
    }
}
