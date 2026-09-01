package io.github.marvinbaudach.reprise

import java.util.Optional
import java.util.concurrent.CountDownLatch
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** Long enough that a loaded host is not mistaken for a read that never ran. */
private const val WAIT_SECONDS = 5L

/** Short enough to stay a unit test, long enough to mean "and then nothing". */
private const val SILENCE_MILLIS = 300L

private fun trackWithId(trackId: Long) = LibraryTrack(
    id = trackId,
    uri = "content://provider/document/$trackId.flac",
    title = "Song $trackId",
    artist = "Artist",
    album = "Album",
    durationMs = 100_000,
    playCount = 3,
    rating = 2,
)

/**
 * The four properties the playing track's row depends on: the read leaves the
 * caller's thread, the answer comes back through the caller's own main-thread
 * hop, an answer for a track that has been superseded is never delivered, and a
 * read that failed is asked again — a bounded number of times.
 *
 * Robolectric because `TrackLoader` logs a failed read, and `android.util.Log`
 * is not a real method without it.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class TrackLoaderTest {
    /**
     * The read happens somewhere else, and the answer does not arrive until the
     * hop runs it. Both halves matter: the first is the whole point — the same
     * library lock a folder scan holds for its entire walk must never be taken
     * where the row is shown — and the second is what lets the caller write
     * Compose state from the answer directly.
     */
    @Test
    fun theReadLeavesTheCallersThreadAndAnswersOnlyThroughTheHop() {
        val hopped = CountDownLatch(1)
        val hops = mutableListOf<() -> Unit>()
        var readingThread: Thread? = null
        val loader = TrackLoader(
            read = { trackId ->
                readingThread = Thread.currentThread()
                trackWithId(trackId)
            },
            onMainThread = { work ->
                hops += work
                hopped.countDown()
            },
        )
        var answered: LibraryTrack? = null

        try {
            loader.load(830) { track -> answered = track }

            assertTrue(hopped.await(WAIT_SECONDS, TimeUnit.SECONDS))
            assertNotEquals(Thread.currentThread(), readingThread)
            assertNull("the answer must not overtake the hop", answered)

            hops.single().invoke()

            assertEquals(trackWithId(830), answered)
        } finally {
            loader.shutdown()
        }
    }

    /**
     * A read that is still running when the next track is asked for answers
     * nobody. The screen wants the *current* row and nothing else, so the reply
     * to "what was playing a moment ago" has nowhere to go — and delivering it
     * anyway would put the previous track under the one now playing.
     */
    @Test
    fun anAnswerIsDroppedOnceAnotherTrackHasBeenAskedFor() {
        val firstReadStarted = CountDownLatch(1)
        val releaseFirstRead = CountDownLatch(1)
        val answered = LinkedBlockingQueue<Long>()
        val loader = TrackLoader(
            read = { trackId ->
                if (trackId == 830L) {
                    firstReadStarted.countDown()
                    releaseFirstRead.await(WAIT_SECONDS, TimeUnit.SECONDS)
                }
                trackWithId(trackId)
            },
            onMainThread = { work -> work() },
        )

        try {
            loader.load(830) { track -> answered.put(track?.id ?: -1) }
            assertTrue(
                "the first read must occupy the lane",
                firstReadStarted.await(WAIT_SECONDS, TimeUnit.SECONDS),
            )

            loader.load(831) { track -> answered.put(track?.id ?: -1) }
            releaseFirstRead.countDown()

            assertEquals(831L, answered.poll(WAIT_SECONDS, TimeUnit.SECONDS))
            assertNull(
                "the superseded read may not answer at all",
                answered.poll(SILENCE_MILLIS, TimeUnit.MILLISECONDS),
            )
        } finally {
            loader.shutdown()
        }
    }

    /**
     * A read that fails once is asked again, and the row arrives.
     *
     * This is the difference between a stumble and a verdict: the row has
     * nobody to report a failure to, so asking again is the only way it can
     * heal. Without it a single busy moment left the playing track without a
     * row for as long as it played.
     */
    @Test
    fun aReadThatFailsOnceIsAskedAgainRatherThanLeavingTheRowEmpty() {
        val attempts = AtomicInteger()
        val waits = LinkedBlockingQueue<Long>()
        val answered = LinkedBlockingQueue<LibraryTrack>()
        val loader = TrackLoader(
            read = { trackId ->
                if (attempts.getAndIncrement() == 0) {
                    throw IllegalStateException("database is busy")
                }
                trackWithId(trackId)
            },
            onMainThread = { work -> work() },
            pauseBeforeRetry = { millis -> waits.put(millis) },
        )

        try {
            loader.load(830) { track -> track?.let(answered::put) }

            assertEquals(trackWithId(830), answered.poll(WAIT_SECONDS, TimeUnit.SECONDS))
            assertEquals(2, attempts.get())
            assertEquals("it waits before asking again", 1, waits.size)
        } finally {
            loader.shutdown()
        }
    }

    /**
     * A read that keeps failing stops asking — and answers with no row.
     *
     * The retry above is bounded on purpose, and this is the bound. A read that
     * fails permanently — a closed handle, a database that is gone — would
     * otherwise lay siege to the very lock the failure is about. Giving up
     * *without* an answer is no better: the screen keeps the row it last got,
     * disabled because it answers for the previous track, and with no answer
     * due it stays stuck there. "No row for this track" is a state it can act
     * on.
     */
    @Test
    fun aReadThatKeepsFailingStopsAskingAndAnswersWithNoRow() {
        val attempts = AtomicInteger()
        val answers = LinkedBlockingQueue<Optional<LibraryTrack>>()
        val waits = LinkedBlockingQueue<Long>()
        val loader = TrackLoader(
            read = { _ ->
                attempts.incrementAndGet()
                throw IllegalStateException("MusicLibrary object has already been destroyed")
            },
            onMainThread = { work -> work() },
            pauseBeforeRetry = { millis -> waits.put(millis) },
        )

        try {
            loader.load(830) { track -> answers.put(Optional.ofNullable(track)) }

            val waited = listOf(
                waits.poll(WAIT_SECONDS, TimeUnit.SECONDS),
                waits.poll(WAIT_SECONDS, TimeUnit.SECONDS),
            )
            assertEquals("each wait is longer than the one before", listOf(50L, 100L), waited)
            assertNull(
                "a read that keeps failing stops rather than waiting to try again",
                waits.poll(SILENCE_MILLIS, TimeUnit.MILLISECONDS),
            )
            assertEquals(3, attempts.get())
            assertEquals(
                "the spent request says so rather than leaving the screen waiting",
                Optional.empty<LibraryTrack>(),
                answers.poll(WAIT_SECONDS, TimeUnit.SECONDS),
            )
            assertNull(
                "and says it once",
                answers.poll(SILENCE_MILLIS, TimeUnit.MILLISECONDS),
            )
        } finally {
            loader.shutdown()
        }
    }

    /**
     * Giving up is still an answer for *one* track: a request that ran out of
     * attempts while the session moved on must not blank the row belonging to
     * the track now playing.
     */
    @Test
    fun aSpentRequestStaysSilentOnceAnotherTrackHasBeenAskedFor() {
        val attempts = AtomicInteger()
        val lastReadStarted = CountDownLatch(1)
        val releaseLastRead = CountDownLatch(1)
        val answers = LinkedBlockingQueue<Optional<LibraryTrack>>()
        val loader = TrackLoader(
            read = { trackId ->
                if (trackId == 830L) {
                    // Held open on the last attempt, so the track can move on
                    // between the final failure and the answer it would give.
                    if (attempts.incrementAndGet() == 3) {
                        lastReadStarted.countDown()
                        releaseLastRead.await(WAIT_SECONDS, TimeUnit.SECONDS)
                    }
                    throw IllegalStateException("database is busy")
                }
                trackWithId(trackId)
            },
            onMainThread = { work -> work() },
            pauseBeforeRetry = { },
        )

        try {
            loader.load(830) { track -> answers.put(Optional.ofNullable(track)) }
            assertTrue(
                "the failing read must reach its last attempt",
                lastReadStarted.await(WAIT_SECONDS, TimeUnit.SECONDS),
            )

            loader.load(831) { track -> answers.put(Optional.ofNullable(track)) }
            releaseLastRead.countDown()

            assertEquals(
                Optional.of(trackWithId(831)),
                answers.poll(WAIT_SECONDS, TimeUnit.SECONDS),
            )
            assertNull(
                "a superseded request may not blank the row it no longer answers for",
                answers.poll(SILENCE_MILLIS, TimeUnit.MILLISECONDS),
            )
        } finally {
            loader.shutdown()
        }
    }
}
