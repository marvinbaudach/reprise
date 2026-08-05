package de.reprise.spike

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.reprise_android_ffi.AndroidTrackAnalysisOutcome

class TrackAnalysisControllerTest {
    @Test
    fun aClosedSheetNeverStartsAnalysis() {
        val backend = RecordingAnalysisBackend()
        val data = RecordingRenderDataStore(hasData = false)
        val controller = TrackAnalysisController(backend, data)

        controller.observe(
            sheetOpen = false,
            trackId = 7,
            contentUri = "content://provider/document/7.flac",
        )
        controller.surfaceActive(true)

        assertEquals(emptyList<Long>(), backend.startedTrackIds)
    }

    @Test
    fun closingTheSheetCancelsTheRunningSession() {
        val backend = RecordingAnalysisBackend()
        val controller = TrackAnalysisController(backend, RecordingRenderDataStore(false))
        controller.surfaceActive(true)
        controller.observe(true, 7, "content://provider/document/7.flac")

        controller.observe(false, 7, "content://provider/document/7.flac")

        assertTrue(backend.works.single().cancelled)
    }

    @Test
    fun changingTrackCancelsTheOldSessionAndStartsTheNewTrack() {
        val backend = RecordingAnalysisBackend()
        val controller = TrackAnalysisController(backend, RecordingRenderDataStore(false))
        controller.surfaceActive(true)
        controller.observe(true, 7, "content://provider/document/7.flac")

        controller.observe(true, 8, "content://provider/document/8.flac")

        assertTrue(backend.works.first().cancelled)
        assertEquals(listOf(7L, 8L), backend.startedTrackIds)
        assertFalse(backend.works.last().cancelled)
    }

    @Test
    fun aStoredAnalysisPublishesNewRenderData() {
        val backend = RecordingAnalysisBackend()
        val data = RecordingRenderDataStore(false)
        val controller = TrackAnalysisController(backend, data)
        controller.surfaceActive(true)
        controller.observe(true, 7, "content://provider/document/7.flac")

        backend.finish(AndroidTrackAnalysisOutcome.STORED)

        assertEquals(listOf(7L), data.storedTrackIds)
    }

    @Test
    fun aChangedSourceIsNotPublishedAsSuccessfulAnalysis() {
        val backend = RecordingAnalysisBackend()
        val data = RecordingRenderDataStore(false)
        val controller = TrackAnalysisController(backend, data)
        controller.surfaceActive(true)
        controller.observe(true, 7, "content://provider/document/7.flac")

        backend.finish(AndroidTrackAnalysisOutcome.SOURCE_CHANGED)

        assertEquals(emptyList<Long>(), data.storedTrackIds)
    }

    @Test
    fun anAnalysedTrackNeverStartsAnotherDecode() {
        val backend = RecordingAnalysisBackend()
        val controller = TrackAnalysisController(backend, RecordingRenderDataStore(true))
        controller.surfaceActive(true)

        controller.observe(true, 7, "content://provider/document/7.flac")

        assertEquals(emptyList<Long>(), backend.startedTrackIds)
    }

    @Test
    fun screenOffAndShutdownBothCancelWithoutKeepingAWorkItemAlive() {
        val backend = RecordingAnalysisBackend()
        val controller = TrackAnalysisController(backend, RecordingRenderDataStore(false))
        controller.surfaceActive(true)
        controller.observe(true, 7, "content://provider/document/7.flac")

        controller.surfaceActive(false)
        assertTrue(backend.works.single().cancelled)

        controller.surfaceActive(true)
        assertEquals(listOf(7L, 7L), backend.startedTrackIds)
        controller.shutdown()

        assertTrue(backend.works.last().cancelled)
    }
}

private class RecordingAnalysisBackend : TrackAnalysisBackend {
    val startedTrackIds = mutableListOf<Long>()
    val works = mutableListOf<RecordingAnalysisWork>()
    private val deliveries = mutableListOf<(TrackAnalysisResult) -> Unit>()

    override fun start(
        trackId: Long,
        contentUri: String,
        deliver: (TrackAnalysisResult) -> Unit,
    ): TrackAnalysisWork {
        startedTrackIds += trackId
        deliveries += deliver
        return RecordingAnalysisWork().also(works::add)
    }

    fun finish(outcome: AndroidTrackAnalysisOutcome) {
        deliveries.last()(Result.success(outcome))
    }
}

private class RecordingAnalysisWork : TrackAnalysisWork {
    var cancelled = false
        private set

    override fun cancel() {
        cancelled = true
    }
}

private class RecordingRenderDataStore(
    private val hasData: Boolean,
) : TrackAnalysisRenderDataStore {
    val storedTrackIds = mutableListOf<Long>()

    override fun hasData(trackId: Long, deliver: (Result<Boolean>) -> Unit) {
        deliver(Result.success(hasData))
    }

    override fun analysisStored(trackId: Long) {
        storedTrackIds += trackId
    }
}
