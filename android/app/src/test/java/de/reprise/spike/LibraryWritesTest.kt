package de.reprise.spike

import java.util.Collections
import java.util.concurrent.CountDownLatch
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/** Long enough that a loaded host is not mistaken for work that never ran. */
private const val WAIT_SECONDS = 5L

/** The production drain bound; a no-answer shutdown must finish inside it. */
private const val DRAIN_TIMEOUT_MS = 2_000L

class LibraryWritesTest {
    @Test
    fun theWriteLeavesTheCallersThread() {
        val caller = Thread.currentThread()
        val writtenOn = LinkedBlockingQueue<Thread>()
        val writes = LibraryWrites(onMainThread = { work -> work() })

        try {
            writes.submitUnanswered(
                work = { writtenOn.put(Thread.currentThread()) },
                onFailure = { throw AssertionError("write failed", it) },
            )

            val worker = writtenOn.poll(WAIT_SECONDS, TimeUnit.SECONDS)
            assertNotEquals(caller, worker)
            assertEquals("reprise-library-writes", worker?.name)
        } finally {
            writes.shutdown()
        }
    }

    @Test
    fun theCallerReturnsBeforeABlockedWriteFinishes() {
        val started = CountDownLatch(1)
        val release = CountDownLatch(1)
        val finished = CountDownLatch(1)
        val writes = LibraryWrites(onMainThread = { work -> work() })

        try {
            writes.submitUnanswered(
                work = {
                    started.countDown()
                    release.await()
                    finished.countDown()
                },
                onFailure = { throw AssertionError("write failed", it) },
            )

            assertTrue(started.await(WAIT_SECONDS, TimeUnit.SECONDS))
            assertEquals("submit returned while the write is blocked", 1L, finished.count)
        } finally {
            release.countDown()
            writes.shutdown()
        }
    }

    @Test
    fun writesReachTheDatabaseInTheOrderTheyWereTapped() {
        val recorded = Collections.synchronizedList(mutableListOf<Int>())
        val completed = CountDownLatch(3)
        val writes = LibraryWrites(onMainThread = { work -> work() })

        try {
            listOf(1, 2, 3).forEach { value ->
                writes.submitUnanswered(
                    work = {
                        recorded += value
                        completed.countDown()
                    },
                    onFailure = { throw AssertionError("write failed", it) },
                )
            }

            assertTrue(completed.await(WAIT_SECONDS, TimeUnit.SECONDS))
            assertEquals(listOf(1, 2, 3), recorded)
        } finally {
            writes.shutdown()
        }
    }

    @Test
    fun anAnsweredWriteIsAnsweredExactlyOnceThroughTheHop() {
        val hops = LinkedBlockingQueue<() -> Unit>()
        val writes = LibraryWrites(onMainThread = { work -> hops.put(work) })
        val answers = mutableListOf<Result<Int>>()

        try {
            writes.submitAnswered(work = { 830 }, report = answers::add)

            val hop = hops.poll(WAIT_SECONDS, TimeUnit.SECONDS)
            assertTrue("the result must reach the main-thread hop", hop != null)
            assertTrue("the answer must not overtake the hop", answers.isEmpty())
            hop?.invoke()

            assertEquals(1, answers.size)
            assertEquals(830, answers.single().getOrThrow())
        } finally {
            writes.shutdown()
        }
    }

    @Test
    fun aFailingWriteIsReportedAndTheLaneKeepsRunning() {
        val refusal = IllegalStateException("database refused the write")
        val answers = LinkedBlockingQueue<Result<Int>>()
        val writes = LibraryWrites(onMainThread = { work -> work() })

        try {
            writes.submitAnswered(work = { throw refusal }, report = answers::put)
            writes.submitAnswered(work = { 2 }, report = answers::put)

            assertSame(refusal, answers.poll(WAIT_SECONDS, TimeUnit.SECONDS)?.exceptionOrNull())
            assertEquals(2, answers.poll(WAIT_SECONDS, TimeUnit.SECONDS)?.getOrThrow())
        } finally {
            writes.shutdown()
        }
    }

    @Test
    fun workSubmittedAfterShutdownIsReportedNotThrown() {
        val writes = LibraryWrites(onMainThread = { work -> work() })
        assertTrue(writes.shutdown())
        var reported: Result<Unit>? = null

        writes.submitAnswered(work = {}, report = { reported = it })

        assertTrue(reported?.exceptionOrNull() is IllegalStateException)
    }

    @Test
    fun shutdownDrainsAnsweredWork() {
        val slowStarted = CountDownLatch(1)
        val releaseSlowWrite = CountDownLatch(1)
        val answered = CountDownLatch(1)
        val writes = LibraryWrites(onMainThread = { work -> work() })
        writes.submitUnanswered(
            work = {
                slowStarted.countDown()
                releaseSlowWrite.await()
            },
            onFailure = { throw AssertionError("write failed", it) },
        )
        writes.submitAnswered(work = { 830 }, report = { answered.countDown() })
        assertTrue(slowStarted.await(WAIT_SECONDS, TimeUnit.SECONDS))
        Thread { releaseSlowWrite.countDown() }.start()

        assertTrue("teardown must drain what was queued", writes.shutdown())
        assertEquals(0L, answered.count)
    }

    @Test
    fun shutdownDoesNotWaitWhenNothingIsWaitingForAnAnswer() {
        val blocked = CountDownLatch(1)
        val neverReleased = CountDownLatch(1)
        val shutdownReturned = CountDownLatch(1)
        val writes = LibraryWrites(onMainThread = { work -> work() })
        writes.submitUnanswered(
            work = {
                blocked.countDown()
                neverReleased.await()
            },
            onFailure = {},
        )
        assertTrue(blocked.await(WAIT_SECONDS, TimeUnit.SECONDS))
        writes.submitUnanswered(work = {}, onFailure = {})

        Thread {
            writes.shutdown()
            shutdownReturned.countDown()
        }.start()

        assertTrue(
            "shutdown must return before the answered-work drain timeout",
            shutdownReturned.await(DRAIN_TIMEOUT_MS, TimeUnit.MILLISECONDS),
        )
        assertFalse("the test must not release the blocked work", neverReleased.count == 0L)
    }
}
