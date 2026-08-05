package de.reprise.spike

import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.reprise_android_ffi.AndroidTrackAnalysisOutcome

class TrackAnalysisBackendTest {
    @Test
    fun cancellingAStartedDecodeActuallyCancelsAndDropsItsSession() {
        val session = RecordingAnalysisSession()
        val decoderEntered = CountDownLatch(1)
        val decoderLeft = CountDownLatch(1)
        val backend = StreamingTrackAnalysisBackend(
            beginSession = { _, _ -> session },
            decoder = TrackPcmDecoder { _, _, cancelled ->
                decoderEntered.countDown()
                while (!cancelled()) Thread.yield()
                decoderLeft.countDown()
            },
            onMainThread = { it() },
        )

        val work = backend.start(7, "content://provider/document/7.flac") {}
        assertTrue(decoderEntered.await(2, TimeUnit.SECONDS))

        work.cancel()

        assertTrue(decoderLeft.await(2, TimeUnit.SECONDS))
        assertEquals(1, session.cancelCount)
        assertEquals(0, session.finishCount)
        assertTrue(session.closed.await(2, TimeUnit.SECONDS))
    }

    @Test
    fun aCompleteDecodePushesPcmAndAnswersBackOnTheMainDispatcher() {
        val session = RecordingAnalysisSession(AndroidTrackAnalysisOutcome.SOURCE_CHANGED)
        val mainAnswers = ArrayDeque<() -> Unit>()
        val outcomes = mutableListOf<TrackAnalysisResult>()
        val backend = StreamingTrackAnalysisBackend(
            beginSession = { _, _ -> session },
            decoder = TrackPcmDecoder { _, sink, _ ->
                sink(listOf(0.25f, -0.25f), 48_000u, 2u)
            },
            onMainThread = { work -> mainAnswers.addLast(work) },
            startWorker = { it() },
        )

        backend.start(9, "content://provider/document/9.flac", outcomes::add)

        assertEquals(1, session.finishCount)
        assertEquals(listOf(listOf(0.25f, -0.25f)), session.pushed)
        assertEquals(emptyList<TrackAnalysisResult>(), outcomes)
        mainAnswers.single().invoke()
        assertEquals(AndroidTrackAnalysisOutcome.SOURCE_CHANGED, outcomes.single().getOrThrow())
    }
}

private class RecordingAnalysisSession(
    private val outcome: AndroidTrackAnalysisOutcome = AndroidTrackAnalysisOutcome.STORED,
) : TrackAnalysisSessionPort {
    val pushed = mutableListOf<List<Float>>()
    val closed = CountDownLatch(1)
    var cancelCount = 0
    var finishCount = 0

    override fun push(interleaved: List<Float>, sampleRateHz: UInt, channelCount: UInt) {
        pushed += interleaved
    }

    override fun finish(): AndroidTrackAnalysisOutcome {
        finishCount += 1
        return outcome
    }

    override fun cancel() {
        cancelCount += 1
    }

    override fun close() {
        closed.countDown()
    }
}
