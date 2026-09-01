package io.github.marvinbaudach.reprise

import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

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
    private val worker: ExecutorService = singleLibraryWriteThread(),
    private val drainTimeoutMs: Long = DRAIN_TIMEOUT_MS,
) {
    private val answeredPending = AtomicInteger()

    /** Persistence nobody is waiting for; failures still return through [onMainThread]. */
    fun submitUnanswered(work: () -> Unit, onFailure: (Throwable) -> Unit) {
        try {
            worker.execute {
                runCatching(work).onFailure { failure ->
                    onMainThread { onFailure(failure) }
                }
            }
        } catch (rejected: RejectedExecutionException) {
            onMainThread {
                onFailure(IllegalStateException(RATING_WRITER_STOPPED, rejected))
            }
        }
    }

    /** The control moves when this answers — exactly once, on the main thread. */
    fun <T> submitAnswered(work: () -> T, report: (Result<T>) -> Unit) {
        answeredPending.incrementAndGet()
        try {
            worker.execute {
                val outcome = runCatching(work)
                onMainThread {
                    try {
                        report(outcome)
                    } finally {
                        answeredPending.decrementAndGet()
                    }
                }
            }
        } catch (rejected: RejectedExecutionException) {
            onMainThread {
                try {
                    report(Result.failure(IllegalStateException(RATING_WRITER_STOPPED, rejected)))
                } finally {
                    answeredPending.decrementAndGet()
                }
            }
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
     * The timeout path deliberately does not call `shutdownNow()`, and doing so
     * would not shorten teardown anyway: the drain only runs out because a
     * write is parked in the library writer inside a JNI call, which
     * `Thread.interrupt()` cannot unblock. All `shutdownNow()` ever bounded was
     * the queued tail — short setter writes that finish in milliseconds once
     * the scan releases the writer — while dropping a queued answered task and
     * stranding its report.
     */
    fun shutdown(): Boolean {
        if (answeredPending.get() == 0) {
            worker.shutdownNow()
            return true
        }

        worker.shutdown()
        val drained = try {
            worker.awaitTermination(drainTimeoutMs, TimeUnit.MILLISECONDS)
        } catch (interrupted: InterruptedException) {
            Thread.currentThread().interrupt()
            false
        }
        return drained
    }
}

private fun singleLibraryWriteThread(): ExecutorService =
    Executors.newSingleThreadExecutor { runnable -> Thread(runnable, "reprise-library-writes") }
