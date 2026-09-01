package io.github.marvinbaudach.reprise

import android.util.Log
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.atomic.AtomicLong

private const val TAG = "RepriseTrack"

/** No track has been asked for yet — no row id can collide with it. */
private const val NOTHING_ASKED_FOR = Long.MIN_VALUE

/**
 * How often one request may reach the database before it gives up.
 *
 * Bounded on purpose. A read that fails once — a busy database, a write
 * commiting underneath it — has to be retried, because otherwise a single
 * stumble leaves the playing track without a row for as long as it plays. A
 * read that keeps failing must **stop**: retrying it on a timer would lay siege
 * to the very lock the failure is about. What it must not do is stop *silently*
 * — see [TrackLoader] for why the giving up is delivered rather than kept.
 */
private const val ATTEMPTS = 3

/** What the first retry waits, doubling for the one after it. */
private const val FIRST_RETRY_MS = 50L

/**
 * The activity's reader for the one row that says what is playing.
 *
 * `track_by_id` no longer waits for `MusicLibrary::scan`, but it is still SQLite
 * I/O. Reading the playing track where it is *shown* would put database work in
 * the composition and risk an ANR. [TrackLoader] keeps that I/O off the main
 * thread; being a read makes it no safer to run inside composition.
 *
 * What differs from [RatingWriter] is everything that follows from being a
 * read:
 *
 * - **A request supersedes the one before it.** The screen only ever wants the
 *   *current* track, so an older read is abandoned rather than delivered — the
 *   answer to "what was playing a moment ago" has nowhere to go. Ratings are
 *   the opposite: every tap is its own intention, and all five have to land in
 *   the order they were made.
 * - **An answer is delivered only while it is still the one asked for.** The
 *   id is checked before the read, before the hop, and again on the main
 *   thread, so a reply that arrives after the track moved on cannot become the
 *   new track's row. `BrowseScreen` keys its state on the id as well; that
 *   check is what makes a stopped session blank rather than stale.
 * - **A failure is retried, [ATTEMPTS] times at most, and then answered.** A
 *   rating that fails is reported to the person who tapped it; a row that fails
 *   to load has nobody to tell, so the only way it can heal is by asking again.
 *   Once the attempts are spent the empty row is delivered all the same:
 *   "there is no row for this track" is a state the screen can act on, whereas
 *   never answering leaves it holding the *previous* track's row with every
 *   action on it disabled, and nothing due that would ever release it.
 * - **Teardown discards rather than drains.** A read that never finishes costs
 *   nothing, exactly as in `TrackArtwork.shutdown`, whose doc comment carries
 *   the full reasoning about why closing the handle underneath a running read
 *   is safe.
 */
internal class TrackLoader(
    private val read: (Long) -> LibraryTrack?,
    private val onMainThread: (() -> Unit) -> Unit,
    private val worker: ExecutorService = singleTrackThread(),
    private val pauseBeforeRetry: (Long) -> Unit = { millis -> Thread.sleep(millis) },
) {
    private val askedFor = AtomicLong(NOTHING_ASKED_FOR)

    /**
     * Asks for one track off the main thread and delivers it on the main
     * thread — but only while it is still the track being asked for.
     *
     * A request that cannot even be queued is dropped rather than answered.
     * That is teardown, and the only honest answer there is the empty row the
     * caller already has: unlike a heart tap, nobody is waiting to be told.
     */
    fun load(trackId: Long, deliver: (LibraryTrack?) -> Unit) {
        askedFor.set(trackId)
        try {
            worker.execute { readAndDeliver(trackId, deliver) }
        } catch (rejected: RejectedExecutionException) {
            Log.d(TAG, "Not loading track $trackId: the library is closing", rejected)
        }
    }

    private fun readAndDeliver(trackId: Long, deliver: (LibraryTrack?) -> Unit) {
        repeat(ATTEMPTS) { attempt ->
            if (!stillWanted(trackId)) return
            // Catching is load-bearing rather than tidy: Android ends the
            // process for an exception that escapes any thread, and one of the
            // failures caught here is teardown itself — a read reaching the
            // library handle after `MainActivity.onDestroy` closed it is
            // refused with `IllegalStateException`.
            val remaining = ATTEMPTS - attempt - 1
            val answered = runCatching { read(trackId) }.fold(
                onSuccess = { track ->
                    // The row the database returned, including "there is no
                    // such row": both are answers, and neither is retried.
                    answer(trackId, track, deliver)
                    true
                },
                onFailure = { error ->
                    Log.w(
                        TAG,
                        "Could not load playing track $trackId, $remaining attempts left",
                        error,
                    )
                    false
                },
            )
            if (answered) return
            if (remaining == 0) {
                // Spent, and said so. The screen keeps the previous track's row
                // until an answer replaces it, with every action on it disabled
                // — so a request that ends without one leaves it stuck there
                // for as long as the track plays.
                answer(trackId, null, deliver)
                return
            }
            try {
                pauseBeforeRetry(FIRST_RETRY_MS shl attempt)
            } catch (interrupted: InterruptedException) {
                Thread.currentThread().interrupt()
                return
            }
        }
    }

    /**
     * Hands one answer over, on the main thread and only while it is still the
     * track being asked for — checked once before the hop and once inside it,
     * because the track can move on while the hop is queued.
     */
    private fun answer(trackId: Long, track: LibraryTrack?, deliver: (LibraryTrack?) -> Unit) {
        if (!stillWanted(trackId)) return
        onMainThread { if (stillWanted(trackId)) deliver(track) }
    }

    private fun stillWanted(trackId: Long): Boolean = askedFor.get() == trackId

    /**
     * Stops reading. It does not wait for the read in progress and must not:
     * see `TrackArtwork.shutdown` for why an abandoned read is safe even while
     * the library handle closes underneath it.
     */
    fun shutdown() {
        worker.shutdownNow()
    }
}

private fun singleTrackThread(): ExecutorService =
    Executors.newSingleThreadExecutor { runnable -> Thread(runnable, "reprise-track") }
