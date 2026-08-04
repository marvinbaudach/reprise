package de.reprise.spike

import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.TimeUnit

/**
 * How long teardown waits for a rating that is already queued.
 *
 * A rating is one indexed `UPDATE` of one row, so the honest expectation is a
 * few milliseconds and this bound is only ever reached by a database something
 * else has wedged. It stays well short of the point where `onDestroy` would
 * look like a hang, because an ordinary rotation goes through that path too.
 */
private const val DRAIN_TIMEOUT_MS = 2_000L

/** What a tap is told when the writer has already stopped. */
internal const val RATING_WRITER_STOPPED = "the library is closing"

/**
 * The activity's rating writer: one thread that owns every star tap.
 *
 * A rating is a SQLite `UPDATE` behind the same handle a SAF scan holds for the
 * whole of its folder walk, so writing it where the tap happens puts the main
 * thread behind a transaction of unbounded length. Play counting was moved off
 * Media3's application thread for the same reason (`play_recorder.rs`); this is
 * the discrete-action half of it.
 *
 * What deliberately does **not** change is *when* the star moves. This is not
 * fire-and-forget: every tap is answered, exactly once, through [rate]'s
 * `report` — and the answer is delivered through [onMainThread] so the caller
 * may write Compose state from it directly. A tap that cannot even be queued is
 * answered too, because a star that neither moves nor explains itself is worse
 * than a slow one.
 *
 * One thread rather than a pool, and that is the load-bearing part: five taps
 * on five stars reach the database in the order they were made, so the rating
 * that survives is the last one tapped rather than whichever write won a race.
 */
internal class RatingWriter(
    private val write: (Long, Int) -> Unit,
    private val onMainThread: (() -> Unit) -> Unit,
    private val worker: ExecutorService = singleRatingThread(),
) {
    /**
     * Queues one rating and answers [report] on the main thread — with the
     * failure the write raised, or success once the database has agreed.
     */
    fun rate(trackId: Long, rating: Int, report: (Result<Unit>) -> Unit) {
        try {
            worker.execute {
                val outcome = runCatching { write(trackId, rating) }
                onMainThread { report(outcome) }
            }
        } catch (rejected: RejectedExecutionException) {
            report(Result.failure(IllegalStateException(RATING_WRITER_STOPPED, rejected)))
        }
    }

    /**
     * Stops accepting taps and waits — briefly — for the ones already queued,
     * reporting whether they all got written.
     *
     * The wait is the point: the caller closes the library handle right after,
     * and a queued write reaching a closed handle would be a crash rather than
     * a lost rating.
     */
    fun shutdown(): Boolean {
        worker.shutdown()
        val drained = try {
            worker.awaitTermination(DRAIN_TIMEOUT_MS, TimeUnit.MILLISECONDS)
        } catch (interrupted: InterruptedException) {
            Thread.currentThread().interrupt()
            false
        }
        if (!drained) {
            worker.shutdownNow()
        }
        return drained
    }
}

private fun singleRatingThread(): ExecutorService =
    Executors.newSingleThreadExecutor { runnable -> Thread(runnable, "reprise-ratings") }
