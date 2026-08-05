package de.reprise.spike

import android.os.Process
import java.util.concurrent.atomic.AtomicBoolean
import uniffi.reprise_android_ffi.AndroidTrackAnalysisOutcome

internal typealias PcmSink = (List<Float>, UInt, UInt) -> Unit

internal fun interface TrackPcmDecoder {
    fun decode(contentUri: String, sink: PcmSink, cancelled: () -> Boolean)
}

internal interface TrackAnalysisSessionPort : AutoCloseable {
    fun push(interleaved: List<Float>, sampleRateHz: UInt, channelCount: UInt)
    fun finish(): AndroidTrackAnalysisOutcome
    fun cancel()
}

/** Runs session creation, decode, pushes, and finalization away from the UI thread. */
internal class StreamingTrackAnalysisBackend(
    private val beginSession: (Long, String) -> TrackAnalysisSessionPort,
    private val decoder: TrackPcmDecoder,
    private val onMainThread: (() -> Unit) -> Unit,
    private val startWorker: ((() -> Unit) -> Unit) = ::startAnalysisWorker,
) : TrackAnalysisBackend {
    override fun start(
        trackId: Long,
        contentUri: String,
        deliver: (TrackAnalysisResult) -> Unit,
    ): TrackAnalysisWork = AnalysisJob(
        trackId = trackId,
        contentUri = contentUri,
        beginSession = beginSession,
        decoder = decoder,
        onMainThread = onMainThread,
        deliver = deliver,
    ).also { job -> startWorker(job::run) }
}

private class AnalysisJob(
    private val trackId: Long,
    private val contentUri: String,
    private val beginSession: (Long, String) -> TrackAnalysisSessionPort,
    private val decoder: TrackPcmDecoder,
    private val onMainThread: (() -> Unit) -> Unit,
    private val deliver: (TrackAnalysisResult) -> Unit,
) : TrackAnalysisWork {
    private val cancelled = AtomicBoolean(false)
    private val sessionCancelled = AtomicBoolean(false)
    private val sessionCall = Any()

    @Volatile
    private var session: TrackAnalysisSessionPort? = null

    override fun cancel() {
        cancelled.set(true)
        cancelSession()
    }

    fun run() {
        if (cancelled.get()) return
        val opened = runCatching { beginSession(trackId, contentUri) }
            .getOrElse { error ->
                answerFailure(error)
                return
            }
        session = opened
        if (cancelled.get()) {
            cancelSession()
            opened.close()
            return
        }

        var finished = false
        val outcome = runCatching {
            decoder.decode(
                contentUri = contentUri,
                sink = { interleaved, sampleRateHz, channelCount ->
                    synchronized(sessionCall) {
                        check(!cancelled.get()) { "track analysis was cancelled" }
                        opened.push(interleaved, sampleRateHz, channelCount)
                    }
                },
                cancelled = cancelled::get,
            )
            synchronized(sessionCall) {
                check(!cancelled.get()) { "track analysis was cancelled" }
                opened.finish().also { finished = true }
            }
        }
        if (!finished) cancelSession()
        opened.close()
        session = null
        if (cancelled.get()) return
        onMainThread { deliver(outcome) }
    }

    private fun cancelSession() {
        val active = session ?: return
        if (sessionCancelled.compareAndSet(false, true)) {
            synchronized(sessionCall) { runCatching(active::cancel) }
        }
    }

    private fun answerFailure(error: Throwable) {
        if (!cancelled.get()) onMainThread { deliver(Result.failure(error)) }
    }
}

private fun startAnalysisWorker(work: () -> Unit) {
    Thread(
        {
            // The platform call is unavailable in local JVM tests; Android
            // accepts it and gives this CPU-heavy pass background priority.
            runCatching { Process.setThreadPriority(Process.THREAD_PRIORITY_BACKGROUND) }
            work()
        },
        "reprise-track-analysis",
    ).apply {
        isDaemon = true
        start()
    }
}
