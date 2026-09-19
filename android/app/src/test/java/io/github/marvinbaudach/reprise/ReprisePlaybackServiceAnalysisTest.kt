package io.github.marvinbaudach.reprise

import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

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
}

private class RecordingAnalysisService : ReprisePlaybackService() {
    val requestedTrackIds = mutableListOf<Long>()

    override fun trackAnalysisRequest(trackId: Long) {
        requestedTrackIds += trackId
    }

    override fun startAnalysisBackfill() = Unit

    override fun cancelAnalysisBackfill() = Unit
}
