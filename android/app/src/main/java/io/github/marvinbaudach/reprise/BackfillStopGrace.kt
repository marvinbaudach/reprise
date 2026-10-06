package io.github.marvinbaudach.reprise

import android.os.Handler

/**
 * How long the library-wide track-analysis backfill outlives play intent
 * before it is cancelled. A cancel throws away the decode in progress and the
 * next run re-decodes the same first pending track from the start, so a
 * pause, a track-change flicker or a buffering blip under load (#1129) must
 * not reach the worker.
 */
internal const val ANALYSIS_BACKFILL_STOP_GRACE_MS = 10_000L

/**
 * A delayed, idempotent stop for the backfill. [schedule] posts [onExpired]
 * once after [graceMs] on the [handler]'s looper; further calls inside the
 * window neither post a second one nor restart the timer. [clear] drops the
 * pending stop, which is what a return to play intent does.
 */
internal class BackfillStopGrace(
    private val handler: Handler,
    private val graceMs: Long,
    private val onExpired: () -> Unit,
) {
    private val expire = Runnable {
        pending = false
        onExpired()
    }
    private var pending = false

    fun schedule() {
        if (pending) return
        pending = true
        handler.postDelayed(expire, graceMs)
    }

    fun clear() {
        if (!pending) return
        pending = false
        handler.removeCallbacks(expire)
    }
}
