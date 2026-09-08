package io.github.marvinbaudach.reprise

import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineName
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeoutOrNull

/** How long teardown waits when a control is waiting for a persistence answer. */
private const val DRAIN_TIMEOUT_MS = 2_000L

/**
 * One ordered background lane for every UI-triggered library write.
 *
 * A SAF scan holds the library writer for its whole folder walk. Calling a
 * short setter where a tap happens can therefore park the main thread behind
 * minutes of provider I/O. This lane leaves the tap thread immediately while
 * preserving write order and returning answered results through [onMainThread].
 *
 * Teardown waits for at most [DRAIN_TIMEOUT_MS] only while answered work is
 * pending, because a control is still waiting for the database to agree. The
 * stopped lane keeps draining after that bound so it cannot strand an answer.
 * With only unanswered persistence queued, teardown drops it immediately
 * instead of making every rotation wait behind a running scan. If answered
 * work is in the same FIFO, earlier unanswered work is drained with it.
 */
internal class LibraryWrites(
    private val onMainThread: (() -> Unit) -> Unit,
    private val dispatcher: CoroutineDispatcher = libraryWriteLane(),
    private val drainTimeoutMs: Long = DRAIN_TIMEOUT_MS,
) {
    private val accepting = AtomicBoolean(true)
    private val answeredPending = AtomicInteger()
    private val job = SupervisorJob()
    private val scope = CoroutineScope(job + dispatcher + CoroutineName("reprise-library-writes"))

    /** Persistence nobody is waiting for; failures still return through [onMainThread]. */
    fun submitUnanswered(work: () -> Unit, onFailure: (Throwable) -> Unit) {
        if (!accepting.get() || !scope.isActive) {
            reject { rejected -> onFailure(rejected) }
            return
        }
        scope.launch {
            try {
                work()
                currentCoroutineContext().ensureActive()
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (failure: Throwable) {
                onMainThread { onFailure(failure) }
            }
        }
    }

    /** The control moves when this answers — exactly once, on the main thread. */
    fun <T> submitAnswered(work: () -> T, report: (Result<T>) -> Unit) {
        answeredPending.incrementAndGet()
        val pendingReturned = AtomicBoolean(false)
        fun returnPending() {
            if (pendingReturned.compareAndSet(false, true)) {
                answeredPending.decrementAndGet()
            }
        }
        if (!accepting.get() || !scope.isActive) {
            try {
                reject { rejected ->
                    try {
                        report(Result.failure(rejected))
                    } finally {
                        returnPending()
                    }
                }
            } catch (failure: Throwable) {
                returnPending()
                throw failure
            }
            return
        }
        val deliveryHandedOff = AtomicBoolean(false)
        val launched = try {
            scope.launch {
                try {
                    val outcome = try {
                        Result.success(work())
                    } catch (cancelled: CancellationException) {
                        throw cancelled
                    } catch (failure: Throwable) {
                        Result.failure(failure)
                    }
                    onMainThread {
                        try {
                            report(outcome)
                        } finally {
                            returnPending()
                        }
                    }
                    deliveryHandedOff.set(true)
                } finally {
                    if (!deliveryHandedOff.get()) returnPending()
                }
            }
        } catch (failure: Throwable) {
            returnPending()
            throw failure
        }
        launched.invokeOnCompletion {
            if (!deliveryHandedOff.get()) returnPending()
        }
    }

    /**
     * Stops new writes before the caller closes the shared library handle.
     *
     * The caller waits briefly for answered work, then leaves the stopped lane
     * to finish it so a queued callback is not stranded after teardown.
     * Unanswered-only work is cancelled at once: losing one stored preference
     * is safer than blocking the main thread behind a folder scan.
     *
     * The timeout path deliberately does not cancel the scope, and doing so
     * would not shorten teardown anyway: the drain only runs out because a
     * write is parked in the library writer inside a JNI call, which coroutine
     * cancellation cannot unblock. All cancellation would bound is the queued
     * tail — short setter writes that finish in milliseconds once the scan
     * releases the writer — while dropping a queued answered task and
     * stranding its report.
     */
    fun shutdown(): Boolean {
        accepting.set(false)
        if (answeredPending.get() == 0) {
            scope.cancel()
            return true
        }

        job.complete()
        val drained = try {
            runBlocking {
                withTimeoutOrNull(drainTimeoutMs) {
                    job.join()
                    true
                } != null
            }
        } catch (interrupted: InterruptedException) {
            Thread.currentThread().interrupt()
            false
        }
        return drained
    }

    private fun reject(report: (IllegalStateException) -> Unit) {
        val rejected = RejectedExecutionException("library writes are shut down")
        onMainThread { report(IllegalStateException(RATING_WRITER_STOPPED, rejected)) }
    }
}

@OptIn(ExperimentalCoroutinesApi::class)
private fun libraryWriteLane(): CoroutineDispatcher = Dispatchers.IO.limitedParallelism(1)
