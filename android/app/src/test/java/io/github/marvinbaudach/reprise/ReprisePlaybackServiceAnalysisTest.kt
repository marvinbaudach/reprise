package io.github.marvinbaudach.reprise

import android.os.Looper
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowLog
import uniffi.reprise_android_ffi.AndroidAnalysisOutcome
import uniffi.reprise_android_ffi.AndroidPlaybackState

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

    @Test
    fun nav_15c_background_playback_changes_enter_analysis_on_the_main_thread() {
        val service = Robolectric.buildService(RecordingAnalysisService::class.java).get()

        Thread {
            service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 41))
        }.also {
            it.start()
            it.join(2_000)
            assertFalse("the playback callback did not return", it.isAlive)
        }

        assertEquals(emptyList<Long>(), service.requestedTrackIds)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf(41L), service.requestedTrackIds)
        assertSame(Looper.getMainLooper().thread, service.requestThreads.single())
    }

    @Test
    fun nav_15c_a_stale_settle_cannot_clear_the_current_request() {
        val service = Robolectric.buildService(RecordingAnalysisService::class.java).get()

        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 41))
        val staleGeneration = service.lastRequestGeneration(41)
        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 7))
        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 41))

        service.settleTrackAnalysis(
            trackId = 41,
            requestGeneration = staleGeneration,
            outcome = AndroidAnalysisOutcome.CANCELLED,
            error = null,
        )
        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 41))

        assertEquals(listOf(41L, 7L, 41L), service.requestedTrackIds)
    }

    @Test
    fun nav_15e_a_track_change_supersedes_the_outgoing_analysis() {
        val service = Robolectric.buildService(RecordingAnalysisService::class.java).get()

        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 41))
        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 41))
        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 7))
        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 7))

        assertEquals(listOf(41L, 7L), service.supersedeKeeps)
    }

    @Test
    fun nav_15e_stopping_playback_supersedes_nothing() {
        val service = Robolectric.buildService(RecordingAnalysisService::class.java).get()

        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 41))
        service.coreListener.onPlaybackChanged(
            m9bSnapshot(trackId = 41).copy(state = AndroidPlaybackState.STOPPED, currentTrackId = null),
        )

        assertEquals(listOf(41L), service.supersedeKeeps)
    }

    @Test
    fun nav_15e_a_switch_then_a_stop_still_supersedes_the_outgoing_track() {
        val service = Robolectric.buildService(SupersedingAnalysisService::class.java).get()
        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 1))
        assertTrue("the first track's supersede never ran", service.awaitSupersede(keep = 1))

        // The supersede keeping 2 cannot run before the stop is delivered.
        val lane = service.holdNextSupersede()
        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 2))
        service.coreListener.onPlaybackChanged(stopped(trackId = 2))
        lane.countDown()

        assertTrue(
            "the stop after the switch left track 1's decode running",
            service.awaitSupersede(keep = 2),
        )
    }

    @Test
    fun nav_15e_quick_switches_supersede_through_the_gate_and_a_stop_spares_the_last_track() {
        val service = Robolectric.buildService(SupersedingAnalysisService::class.java).get()
        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 1))
        assertTrue("the first track's supersede never ran", service.awaitSupersede(keep = 1))

        val lane = service.holdNextSupersede()
        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 2))
        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 3))
        service.coreListener.onPlaybackChanged(stopped(trackId = 3))
        lane.countDown()

        assertTrue("the last track's supersede never ran", service.awaitSupersede(keep = 3))
        assertEquals(
            "the call keeping 2 reached the library and stopped 3",
            listOf(1L, 3L),
            service.superseded.toList(),
        )
    }

    @Test
    fun nav_15e_a_superseded_settle_for_the_playing_track_requests_again() {
        val service = Robolectric.buildService(RecordingAnalysisService::class.java).get()

        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 41))
        // A stale supersede reached the track that is playing now.
        service.settle(41, AndroidAnalysisOutcome.SUPERSEDED)
        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 41))

        assertEquals(listOf(41L, 41L), service.requestedTrackIds)
    }

    @Test
    fun nav_15e_a_request_that_starts_after_its_track_lost_its_place_imports_nothing() {
        val service = Robolectric.buildService(ImportRecordingService::class.java).get()
        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 7))
        assertTrue("the playing track was never imported", service.awaitImport(7))

        // The request for 41 was posted before the switch to 7 and starts only now.
        service.trackAnalysisRequest(trackId = 41, requestGeneration = 0L)

        assertTrue("the stale request never finished", awaitSkippedRequest(41))
        assertFalse("a track nobody plays any more started a decode", service.imported(41))
    }

    @Test
    fun nav_15e_a_request_for_a_track_left_before_a_stop_imports_nothing() {
        val service = Robolectric.buildService(ImportRecordingService::class.java).get()
        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 7))
        assertTrue("the playing track was never imported", service.awaitImport(7))
        service.coreListener.onPlaybackChanged(stopped(trackId = 7))

        // Posted while 41 played, before the switch to 7 and the stop.
        service.trackAnalysisRequest(trackId = 41, requestGeneration = 0L)

        assertTrue("the stale request never finished", awaitSkippedRequest(41))
        assertFalse("a track left for another started a decode after the stop", service.imported(41))
    }

    @Test
    fun nav_15e_a_request_for_the_track_a_stop_left_still_imports() {
        val service = Robolectric.buildService(ImportRecordingService::class.java).get()
        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 7))
        assertTrue("the playing track was never imported", service.awaitImport(7))
        service.coreListener.onPlaybackChanged(stopped(trackId = 7))

        // A request for 7 that only starts after the stop.
        service.trackAnalysisRequest(trackId = 7, requestGeneration = 0L)

        assertTrue("a stop dropped the stopped track's analysis", service.awaitImport(7, imports = 2))
    }

    @Test
    fun nav_15c_a_real_failed_request_posts_its_retry_state_to_main() {
        val service = Robolectric.buildService(ImportingAnalysisService::class.java).get()

        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 41))
        assertTrue("the first import never ran", service.awaitImport(1))
        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(2)
        while (!service.firstSettlementDelivered() && System.nanoTime() < deadline) {
            shadowOf(Looper.getMainLooper()).idle()
            Thread.yield()
        }
        assertTrue("the first import never settled", service.firstSettlementDelivered())
        assertSame(Looper.getMainLooper().thread, service.settlementThreads.single())
        service.coreListener.onPlaybackChanged(m9bSnapshot(trackId = 41))

        assertTrue("the posted failure did not permit a retry", service.awaitImport(2))
        assertEquals(2, service.imports.get())
    }
}

private const val TEST_TIMEOUT_SECONDS = 2L
private const val ANALYSIS_LOG_TAG = "RepriseAnalysis"

private fun stopped(trackId: Long) =
    m9bSnapshot(trackId = trackId).copy(state = AndroidPlaybackState.STOPPED, currentTrackId = null)

/** The service logs the moment it drops a request whose track lost its place. */
private fun awaitSkippedRequest(trackId: Long): Boolean {
    val line = "Skipping the analysis request for track $trackId"
    val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(TEST_TIMEOUT_SECONDS)
    while (System.nanoTime() < deadline) {
        if (ShadowLog.getLogsForTag(ANALYSIS_LOG_TAG).any { it.msg.startsWith(line) }) return true
        Thread.yield()
    }
    return false
}

private class ImportRecordingService : ReprisePlaybackService() {
    private val imports = java.util.concurrent.ConcurrentHashMap<Long, AtomicInteger>()

    private fun importsOf(trackId: Long) = imports.getOrPut(trackId) { AtomicInteger(0) }

    override fun importTrackAnalysis(trackId: Long): AndroidAnalysisOutcome {
        importsOf(trackId).incrementAndGet()
        return AndroidAnalysisOutcome.COMPUTED
    }

    fun imported(trackId: Long): Boolean = importsOf(trackId).get() > 0

    fun awaitImport(trackId: Long, imports: Int = 1): Boolean {
        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(TEST_TIMEOUT_SECONDS)
        while (importsOf(trackId).get() < imports && System.nanoTime() < deadline) Thread.yield()
        return importsOf(trackId).get() >= imports
    }

    override fun settleTrackAnalysis(
        trackId: Long,
        requestGeneration: Long,
        outcome: AndroidAnalysisOutcome?,
        error: Throwable?,
    ) = Unit

    override fun supersedeForegroundAnalysis(keepTrackId: Long) = Unit

    override fun startAnalysisBackfill() = Unit

    override fun cancelAnalysisBackfill() = Unit
}

/** Records the supersedes that reach the library, and can hold the serial lane. */
private class SupersedingAnalysisService : ReprisePlaybackService() {
    private val reached = java.util.concurrent.ConcurrentHashMap<Long, CountDownLatch>()
    val superseded = java.util.concurrent.CopyOnWriteArrayList<Long>()

    @Volatile
    private var hold: CountDownLatch? = null

    private fun reachedFor(keep: Long) = reached.getOrPut(keep) { CountDownLatch(1) }

    /**
     * Holds the next supersede job before it enters the gate, so the track
     * changes delivered meanwhile reach the gate first; count it down to go on.
     */
    fun holdNextSupersede(): CountDownLatch = CountDownLatch(1).also { hold = it }

    override fun foregroundAnalysisSuperseder(): (Long) -> Unit {
        hold?.let { latch ->
            hold = null
            latch.await(TEST_TIMEOUT_SECONDS, TimeUnit.SECONDS)
        }
        return { keep ->
            superseded += keep
            reachedFor(keep).countDown()
        }
    }

    fun awaitSupersede(keep: Long): Boolean = reachedFor(keep).await(TEST_TIMEOUT_SECONDS, TimeUnit.SECONDS)

    override fun trackAnalysisRequest(trackId: Long, requestGeneration: Long) = Unit

    override fun startAnalysisBackfill() = Unit

    override fun cancelAnalysisBackfill() = Unit
}

private class ImportingAnalysisService : ReprisePlaybackService() {
    val imports = AtomicInteger(0)
    val settlementThreads = mutableListOf<Thread>()
    private val imported = listOf(CountDownLatch(1), CountDownLatch(1))
    private val firstSettlement = CountDownLatch(1)

    override fun importTrackAnalysis(trackId: Long): AndroidAnalysisOutcome {
        val attempt = imports.incrementAndGet()
        imported[attempt - 1].countDown()
        if (attempt == 1) error("decoder stopped")
        return AndroidAnalysisOutcome.COMPUTED
    }

    override fun settleTrackAnalysis(
        trackId: Long,
        requestGeneration: Long,
        outcome: AndroidAnalysisOutcome?,
        error: Throwable?,
    ) {
        settlementThreads += Thread.currentThread()
        super.settleTrackAnalysis(trackId, requestGeneration, outcome, error)
        firstSettlement.countDown()
    }

    override fun supersedeForegroundAnalysis(keepTrackId: Long) = Unit

    fun awaitImport(attempt: Int): Boolean = imported[attempt - 1].await(2, TimeUnit.SECONDS)

    fun firstSettlementDelivered(): Boolean = firstSettlement.count == 0L

    override fun startAnalysisBackfill() = Unit

    override fun cancelAnalysisBackfill() = Unit
}

private class RecordingAnalysisService : ReprisePlaybackService() {
    private val requests = mutableListOf<Pair<Long, Long>>()
    val requestedTrackIds: List<Long>
        get() = requests.map(Pair<Long, Long>::first)
    val requestThreads = mutableListOf<Thread>()
    val supersedeKeeps = mutableListOf<Long>()

    override fun supersedeForegroundAnalysis(keepTrackId: Long) {
        supersedeKeeps += keepTrackId
    }

    override fun trackAnalysisRequest(trackId: Long, requestGeneration: Long) {
        requestThreads += Thread.currentThread()
        requests += trackId to requestGeneration
    }

    fun lastRequestGeneration(trackId: Long): Long =
        requests.last { (requestedTrackId, _) -> requestedTrackId == trackId }.second

    fun settle(trackId: Long, outcome: AndroidAnalysisOutcome) {
        val requestGeneration = requests.last { (requestedTrackId, _) ->
            requestedTrackId == trackId
        }.second
        settleTrackAnalysis(trackId, requestGeneration, outcome, error = null)
    }

    override fun startAnalysisBackfill() = Unit

    override fun cancelAnalysisBackfill() = Unit
}
