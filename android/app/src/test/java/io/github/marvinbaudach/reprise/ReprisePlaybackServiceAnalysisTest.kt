package io.github.marvinbaudach.reprise

import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidAnalysisOutcome

/**
 * The service's own request-on-change: `import_track_analysis` runs for the
 * current track on every track change, even with no activity attached
 * (decision 5 of `docs/plans/the-phone-analyses-its-own-music.md`) — and
 * never again for the same track on a mere position tick.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class ReprisePlaybackServiceAnalysisTest {
    @Test
    fun requestsAnalysisOncePerTrackChangeNeverOnAPositionTick() {
        val service = Robolectric.buildService(RecordingAnalysisService::class.java).get()

        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 41))
        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 41))
        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 41))
        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 7))
        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 7))

        assertEquals(listOf(41L, 7L), service.requestedTrackIds)
    }

    @Test
    fun nav_15c_a_non_final_result_allows_the_current_track_to_request_again() {
        val service = Robolectric.buildService(RecordingAnalysisService::class.java).get()

        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 41))
        service.settle(41, AndroidAnalysisOutcome.CANCELLED)
        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 41))

        assertEquals(listOf(41L, 41L), service.requestedTrackIds)
    }

    @Test
    fun analysisStopsAfterThreeNonFinalRequestsForOneTrack() {
        val service = Robolectric.buildService(RecordingAnalysisService::class.java).get()

        repeat(MAX_ANALYSIS_ATTEMPTS + 1) {
            service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 41))
            service.settle(41, AndroidAnalysisOutcome.PHONE_SOURCE_CHANGED)
        }

        assertEquals(listOf(41L, 41L, 41L), service.requestedTrackIds)
    }

    @Test
    fun everyFinalAnalysisOutcomeStopsRequestsForTheCurrentTrack() {
        listOf(
            AndroidAnalysisOutcome.COMPUTED,
            AndroidAnalysisOutcome.DECODE_FAILED,
        ).forEach { outcome ->
            val service = Robolectric.buildService(RecordingAnalysisService::class.java).get()

            service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 41))
            service.settle(41, outcome)
            service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 41))

            assertEquals(listOf(41L), service.requestedTrackIds)
        }
    }

    @Test
    fun aTrackNeverHasTwoAnalysisRequestsInFlight() {
        val service = Robolectric.buildService(RecordingAnalysisService::class.java).get()

        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 41))
        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 41))
        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 41))

        assertEquals(listOf(41L), service.requestedTrackIds)
    }

    @Test
    fun changingTrackResetsTheAnalysisAttemptCounter() {
        val service = Robolectric.buildService(RecordingAnalysisService::class.java).get()

        repeat(MAX_ANALYSIS_ATTEMPTS) {
            service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 41))
            service.settle(41, AndroidAnalysisOutcome.CANCELLED)
        }
        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 7))

        assertEquals(listOf(41L, 41L, 41L, 7L), service.requestedTrackIds)
    }
}

private class RecordingAnalysisService : ReprisePlaybackService() {
    private val requests = mutableListOf<Pair<Long, Long>>()
    val requestedTrackIds: List<Long>
        get() = requests.map(Pair<Long, Long>::first)

    override fun trackAnalysisRequest(trackId: Long, requestGeneration: Long) {
        requests += trackId to requestGeneration
    }

    fun settle(trackId: Long, outcome: AndroidAnalysisOutcome) {
        val requestGeneration = requests.last { (requestedTrackId, _) ->
            requestedTrackId == trackId
        }.second
        settleTrackAnalysis(trackId, requestGeneration, outcome, error = null)
    }

    override fun startAnalysisBackfill() = Unit

    override fun cancelAnalysisBackfill() = Unit
}
