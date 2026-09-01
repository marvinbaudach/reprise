package io.github.marvinbaudach.reprise

import java.util.Collections
import java.util.concurrent.AbstractExecutorService
import java.util.concurrent.CountDownLatch
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference
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
    @Test(timeout = 10_000)
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
        } finally {
            writes.shutdown()
        }
    }

    @Test(timeout = 10_000)
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

    @Test(timeout = 10_000)
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

    @Test(timeout = 10_000)
    fun aFailingUnansweredWriteReportsThroughTheMainHopAndTheLaneKeepsRunning() {
        val refusal = IllegalStateException("database refused the write")
        val hops = LinkedBlockingQueue<() -> Unit>()
        val failures = mutableListOf<Throwable>()
        val nextWrite = CountDownLatch(1)
        val writes = LibraryWrites(onMainThread = hops::put)

        try {
            writes.submitUnanswered(
                work = { throw refusal },
                onFailure = failures::add,
            )
            writes.submitUnanswered(
                work = nextWrite::countDown,
                onFailure = { throw AssertionError("next write failed", it) },
            )

            val hop = hops.poll(WAIT_SECONDS, TimeUnit.SECONDS)
            assertTrue("the failure must reach the main-thread hop", hop != null)
            assertTrue("the failure must not overtake the hop", failures.isEmpty())
            hop?.invoke()
            assertEquals(listOf(refusal), failures)
            assertTrue("the lane must keep running", nextWrite.await(WAIT_SECONDS, TimeUnit.SECONDS))
        } finally {
            writes.shutdown()
        }
    }

    @Test(timeout = 10_000)
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
            assertTrue("exactly one main-thread hop must be dispatched", hops.isEmpty())
        } finally {
            writes.shutdown()
        }
    }

    @Test(timeout = 10_000)
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

    @Test(timeout = 10_000)
    fun workSubmittedAfterShutdownIsReportedNotThrown() {
        val writes = LibraryWrites(onMainThread = { work -> work() })
        assertTrue(writes.shutdown())
        var reported: Result<Unit>? = null

        writes.submitAnswered(work = {}, report = { reported = it })

        assertTrue(reported?.exceptionOrNull() is IllegalStateException)
    }

    @Test(timeout = 10_000)
    fun unansweredWorkSubmittedAfterShutdownReportsThroughTheMainHop() {
        val hops = LinkedBlockingQueue<() -> Unit>()
        val failures = mutableListOf<Throwable>()
        val writes = LibraryWrites(onMainThread = hops::put)
        assertTrue(writes.shutdown())

        writes.submitUnanswered(work = {}, onFailure = failures::add)

        assertTrue("rejection must not call the failure hook directly", failures.isEmpty())
        val hop = hops.poll(WAIT_SECONDS, TimeUnit.SECONDS)
        assertTrue("rejection must reach the main-thread hop", hop != null)
        hop?.invoke()
        assertTrue(failures.single() is IllegalStateException)
        assertTrue(failures.single().cause is RejectedExecutionException)
    }

    @Test(timeout = 10_000)
    fun rejectedAnswerReturnsPendingToZeroBeforeShutdown() {
        val worker = RejectingExecutorService()
        val writes = LibraryWrites(onMainThread = { work -> work() }, worker = worker)
        var reported: Result<Unit>? = null

        writes.submitAnswered(work = {}, report = { reported = it })

        assertTrue(reported?.exceptionOrNull() is IllegalStateException)
        assertTrue(writes.shutdown())
        assertEquals(1, worker.immediateStops)
        assertEquals(0, worker.drains)
    }

    @Test(timeout = 10_000)
    fun shutdownDrainsAnsweredWork() {
        val slowStarted = CountDownLatch(1)
        val releaseSlowWrite = CountDownLatch(1)
        val answered = CountDownLatch(1)
        val worker = Executors.newSingleThreadExecutor()
        val writes = LibraryWrites(onMainThread = { work -> work() }, worker = worker)
        writes.submitUnanswered(
            work = {
                slowStarted.countDown()
                releaseSlowWrite.await()
            },
            onFailure = { throw AssertionError("write failed", it) },
        )
        writes.submitAnswered(work = { 830 }, report = { answered.countDown() })
        assertTrue(slowStarted.await(WAIT_SECONDS, TimeUnit.SECONDS))
        val shutdownResult = LinkedBlockingQueue<Boolean>()
        Thread { shutdownResult.put(writes.shutdown()) }.start()
        assertTrue("shutdown must enter the answered drain branch", worker.awaitShutdown())
        releaseSlowWrite.countDown()

        assertTrue("teardown must drain what was queued", shutdownResult.poll(WAIT_SECONDS, TimeUnit.SECONDS) == true)
        assertEquals(0L, answered.count)
    }

    @Test(timeout = 10_000)
    fun answeredPendingReturnsToZeroAfterTheReportIsHandedOver() {
        val answered = CountDownLatch(1)
        val blocked = CountDownLatch(1)
        val release = CountDownLatch(1)
        val writes = LibraryWrites(
            onMainThread = { work -> work() },
            drainTimeoutMs = 10,
        )
        try {
            writes.submitAnswered(work = {}, report = { answered.countDown() })
            assertTrue(answered.await(WAIT_SECONDS, TimeUnit.SECONDS))
            writes.submitUnanswered(
                work = {
                    blocked.countDown()
                    release.await()
                },
                onFailure = {},
            )
            assertTrue(blocked.await(WAIT_SECONDS, TimeUnit.SECONDS))

            assertTrue("completed answers must restore the immediate shutdown path", writes.shutdown())
        } finally {
            release.countDown()
        }
    }

    @Test(timeout = 10_000)
    fun shutdownTimeoutKeepsQueuedAnsweredWorkForALateReport() {
        val slowStarted = CountDownLatch(1)
        val releaseSlowWrite = CountDownLatch(1)
        val answers = LinkedBlockingQueue<Result<Int>>()
        val writes = LibraryWrites(
            onMainThread = { work -> work() },
            drainTimeoutMs = 10,
        )
        writes.submitUnanswered(
            work = {
                slowStarted.countDown()
                releaseSlowWrite.await()
            },
            onFailure = { throw AssertionError("write failed", it) },
        )
        writes.submitAnswered(work = { 830 }, report = answers::put)
        assertTrue(slowStarted.await(WAIT_SECONDS, TimeUnit.SECONDS))

        assertFalse("the blocked lane must exhaust the drain bound", writes.shutdown())
        releaseSlowWrite.countDown()

        assertEquals(830, answers.poll(WAIT_SECONDS, TimeUnit.SECONDS)?.getOrThrow())
        assertTrue("the late report must still be exactly once", answers.isEmpty())
    }

    @Test(timeout = 10_000)
    fun interruptedDrainReturnsFalseAndKeepsQueuedAnsweredWorkForALateReport() {
        val slowStarted = CountDownLatch(1)
        val releaseSlowWrite = CountDownLatch(1)
        val answers = LinkedBlockingQueue<Result<Int>>()
        val worker = Executors.newSingleThreadExecutor()
        val writes = LibraryWrites(onMainThread = { work -> work() }, worker = worker)
        writes.submitUnanswered(
            work = {
                slowStarted.countDown()
                releaseSlowWrite.await()
            },
            onFailure = { throw AssertionError("write failed", it) },
        )
        writes.submitAnswered(work = { 830 }, report = answers::put)
        assertTrue(slowStarted.await(WAIT_SECONDS, TimeUnit.SECONDS))
        val shutdownResult = AtomicReference<Boolean>()
        val interruptPreserved = AtomicReference<Boolean>()
        val shutdownReturned = CountDownLatch(1)
        val shutdownThread = Thread {
            shutdownResult.set(writes.shutdown())
            interruptPreserved.set(Thread.currentThread().isInterrupted)
            shutdownReturned.countDown()
        }.also(Thread::start)
        assertTrue("shutdown must be awaiting the answered drain", worker.awaitShutdown())

        shutdownThread.interrupt()

        assertTrue(shutdownReturned.await(WAIT_SECONDS, TimeUnit.SECONDS))
        assertFalse(shutdownResult.get())
        assertTrue(interruptPreserved.get())
        releaseSlowWrite.countDown()
        assertEquals(830, answers.poll(WAIT_SECONDS, TimeUnit.SECONDS)?.getOrThrow())
    }

    @Test(timeout = 10_000)
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
    }
}

private fun ExecutorService.awaitShutdown(): Boolean {
    val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(WAIT_SECONDS)
    while (!isShutdown && System.nanoTime() < deadline) {
        Thread.sleep(1)
    }
    return isShutdown
}

private class RejectingExecutorService : AbstractExecutorService() {
    var drains = 0
    var immediateStops = 0

    override fun execute(command: Runnable) {
        throw RejectedExecutionException("test rejection")
    }

    override fun shutdown() {
        drains += 1
    }

    override fun shutdownNow(): MutableList<Runnable> {
        immediateStops += 1
        return mutableListOf()
    }

    override fun isShutdown(): Boolean = false

    override fun isTerminated(): Boolean = false

    override fun awaitTermination(timeout: Long, unit: TimeUnit): Boolean = false
}
