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
 * lane is cancelled after that bound so teardown itself stays bounded.
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
                currentCoroutineContext().ensureActive()
                onMainThread { onFailure(failure) }
            }
        }
    }

    /** The control moves when this answers — exactly once, on the main thread. */
    fun <T> submitAnswered(work: () -> T, report: (Result<T>) -> Unit) {
        answeredPending.incrementAndGet()
        if (!accepting.get() || !scope.isActive) {
            reject { rejected ->
                try {
                    report(Result.failure(rejected))
                } finally {
                    answeredPending.decrementAndGet()
                }
            }
            return
        }
        scope.launch {
            val outcome = try {
                Result.success(work())
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (failure: Throwable) {
                currentCoroutineContext().ensureActive()
                Result.failure(failure)
            }
            currentCoroutineContext().ensureActive()
            onMainThread {
                try {
                    report(outcome)
                } finally {
                    answeredPending.decrementAndGet()
                }
            }
        }
    }

    /**
     * Stops new writes before the caller closes the shared library handle.
     *
     * The caller waits briefly for answered work, then cancels the stopped lane
     * if the bound expires.
     * Unanswered-only work is cancelled at once: losing one stored preference
     * is safer than blocking the main thread behind a folder scan.
     *
     * Cancelling cannot interrupt a write parked in JNI, but it discards the
     * queued tail and prevents a completed in-flight call from delivering into
     * an activity that has already been destroyed.
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
        if (!drained) scope.cancel()
        return drained
    }

    private fun reject(report: (IllegalStateException) -> Unit) {
        val rejected = RejectedExecutionException("library writes are shut down")
        onMainThread { report(IllegalStateException(RATING_WRITER_STOPPED, rejected)) }
    }
}

@OptIn(ExperimentalCoroutinesApi::class)
private fun libraryWriteLane(): CoroutineDispatcher = Dispatchers.IO.limitedParallelism(1)
