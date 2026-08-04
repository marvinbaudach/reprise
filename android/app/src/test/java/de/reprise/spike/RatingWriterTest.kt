package de.reprise.spike

import java.util.concurrent.CountDownLatch
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Long enough that a loaded host is not mistaken for a writer that never ran. */
private const val WAIT_SECONDS = 5L

/**
 * The four properties the star tap depends on: it leaves the caller's thread,
 * its answer comes back through the caller's own main-thread hop, taps are
 * written in the order they were made, and no tap is ever left unanswered.
 */
class RatingWriterTest {
    /**
     * The write happens somewhere else, and the answer does not arrive until
     * the hop runs it. Both halves matter: the first is the whole point, and
     * the second is what lets the caller write Compose state from `report`
     * without a second thought.
     */
    @Test
    fun theWriteLeavesTheCallersThreadAndAnswersOnlyThroughTheHop() {
        val hopped = CountDownLatch(1)
        val hops = mutableListOf<() -> Unit>()
        var writingThread: Thread? = null
        val writer = RatingWriter(
            write = { _, _ -> writingThread = Thread.currentThread() },
            onMainThread = { work ->
                hops += work
                hopped.countDown()
            },
        )
        var answered: Result<Unit>? = null

        try {
            writer.rate(trackId = 830, rating = 4) { outcome -> answered = outcome }

            assertTrue(hopped.await(WAIT_SECONDS, TimeUnit.SECONDS))
            assertNotEquals(Thread.currentThread(), writingThread)
            assertNull("the answer must not overtake the hop", answered)

            hops.single().invoke()

            assertEquals(true, answered?.isSuccess)
        } finally {
            writer.shutdown()
        }
    }

    /**
     * A rating the database refused comes back as that refusal, unchanged. This
     * is the failure the sheet turns into its message, so losing it here would
     * turn a reported failure into a silent one.
     */
    @Test
    fun aWriteThatThrowsComesBackAsTheFailureRatherThanAsSilence() {
        val refusal = IllegalStateException("track is missing")
        val answers = LinkedBlockingQueue<Result<Unit>>()
        val writer = RatingWriter(
            write = { _, _ -> throw refusal },
            onMainThread = { work -> work() },
        )

        try {
            writer.rate(trackId = 830, rating = 4) { outcome -> answers.put(outcome) }

            val answered = answers.poll(WAIT_SECONDS, TimeUnit.SECONDS)
            assertEquals(refusal, answered?.exceptionOrNull())
        } finally {
            writer.shutdown()
        }
    }

    /**
     * Five taps on five stars are one intention with an order, not five racing
     * writes: the rating left in the database has to be the last one tapped.
     * The same run proves teardown drains what was queued rather than dropping
     * it.
     */
    @Test
    fun tapsAreWrittenAndAnsweredInTheOrderTheyWereMade() {
        val written = LinkedBlockingQueue<Int>()
        val answered = LinkedBlockingQueue<Int>()
        val writer = RatingWriter(
            write = { _, rating -> written.put(rating) },
            onMainThread = { work -> work() },
        )

        try {
            (1..5).forEach { star ->
                writer.rate(trackId = 830, rating = star) { answered.put(star) }
            }

            assertTrue("teardown must drain what was queued", writer.shutdown())
            assertEquals(listOf(1, 2, 3, 4, 5), written.toList())
            assertEquals(listOf(1, 2, 3, 4, 5), answered.toList())
        } finally {
            writer.shutdown()
        }
    }

    /**
     * The one tap that cannot be queued at all is still answered. A star that
     * neither moves nor says why is the failure mode this whole path exists to
     * avoid.
     */
    @Test
    fun aTapMadeAfterTheWriterStoppedIsAnsweredRatherThanDropped() {
        var writes = 0
        val writer = RatingWriter(
            write = { _, _ -> writes += 1 },
            onMainThread = { work -> work() },
        )
        assertTrue(writer.shutdown())
        var answered: Result<Unit>? = null

        writer.rate(trackId = 830, rating = 4) { outcome -> answered = outcome }

        assertEquals(0, writes)
        assertEquals(false, answered?.isSuccess)
        assertEquals(RATING_WRITER_STOPPED, answered?.exceptionOrNull()?.message)
    }
}
