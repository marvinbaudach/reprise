package io.github.marvinbaudach.reprise

import android.content.Context
import android.os.Looper
import android.os.PowerManager
import java.time.Duration
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidPlaybackSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackState

/**
 * When the library-wide backfill is cancelled (#1129). A cancel discards the
 * decode in progress, so only a real departure from play intent may reach the
 * worker: a buffering blip never does, and any other departure is given
 * `ANALYSIS_BACKFILL_STOP_GRACE_MS` first.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class ReprisePlaybackServiceBackfillTest {
    private val looper = shadowOf(Looper.getMainLooper())

    @Test
    fun a_buffering_blip_keeps_the_backfill_running() {
        val service = Robolectric.buildService(BackfillRecordingService::class.java).get()

        service.report(AndroidPlaybackState.PLAYING)
        service.report(AndroidPlaybackState.BUFFERING)
        service.report(AndroidPlaybackState.PLAYING)
        service.report(AndroidPlaybackState.BUFFERING)
        // Past the grace period: play intent never lapsed, so nothing is pending.
        looper.idleFor(Duration.ofMillis(ANALYSIS_BACKFILL_STOP_GRACE_MS * 2))

        assertEquals(listOf("start"), service.calls)
    }

    @Test
    fun a_short_pause_keeps_the_backfill_running() {
        val service = Robolectric.buildService(BackfillRecordingService::class.java).get()

        service.report(AndroidPlaybackState.PLAYING)
        service.report(AndroidPlaybackState.PAUSED)
        looper.idleFor(Duration.ofSeconds(5))
        service.report(AndroidPlaybackState.PLAYING)
        looper.idleFor(Duration.ofMillis(ANALYSIS_BACKFILL_STOP_GRACE_MS * 2))

        assertEquals(listOf("start"), service.calls)
    }

    @Test
    fun a_pause_longer_than_the_grace_cancels_the_backfill() {
        val service = Robolectric.buildService(BackfillRecordingService::class.java).get()

        service.report(AndroidPlaybackState.PLAYING)
        service.report(AndroidPlaybackState.PAUSED)
        looper.idleFor(Duration.ofMillis(ANALYSIS_BACKFILL_STOP_GRACE_MS - 1))
        assertEquals(listOf("start"), service.calls)
        looper.idleFor(Duration.ofMillis(1))

        assertEquals(listOf("start", "cancel"), service.calls)
    }

    @Test
    fun two_pause_snapshots_inside_the_grace_cancel_once() {
        val service = Robolectric.buildService(BackfillRecordingService::class.java).get()

        service.report(AndroidPlaybackState.PLAYING)
        service.report(AndroidPlaybackState.PAUSED)
        looper.idleFor(Duration.ofSeconds(6))
        service.report(AndroidPlaybackState.STOPPED)
        // The second snapshot did not restart the timer: 10 s after the first.
        looper.idleFor(Duration.ofSeconds(4))
        assertEquals(listOf("start", "cancel"), service.calls)
        looper.idleFor(Duration.ofSeconds(30))

        assertEquals(listOf("start", "cancel"), service.calls)
    }

    @Test
    fun battery_saver_cancels_the_backfill_at_once() {
        val service = Robolectric.buildService(BackfillRecordingService::class.java).get()

        service.report(AndroidPlaybackState.PLAYING)
        powerManager(service).setIsPowerSaveMode(true)
        service.report(AndroidPlaybackState.PLAYING)

        assertEquals(listOf("start", "cancel"), service.calls)
    }

    @Test
    fun battery_saver_drops_a_pending_grace_so_it_cancels_only_once() {
        val service = Robolectric.buildService(BackfillRecordingService::class.java).get()

        service.report(AndroidPlaybackState.PLAYING)
        service.report(AndroidPlaybackState.PAUSED)
        powerManager(service).setIsPowerSaveMode(true)
        service.report(AndroidPlaybackState.PAUSED)
        looper.idleFor(Duration.ofMillis(ANALYSIS_BACKFILL_STOP_GRACE_MS * 2))

        assertEquals(listOf("start", "cancel"), service.calls)
    }

    @Test
    fun destroying_the_service_drops_a_pending_cancel() {
        val service = Robolectric.buildService(BackfillRecordingService::class.java).get()

        service.report(AndroidPlaybackState.PLAYING)
        service.report(AndroidPlaybackState.PAUSED)
        runCatching { service.onDestroy() }
        looper.idleFor(Duration.ofMillis(ANALYSIS_BACKFILL_STOP_GRACE_MS * 2))

        assertEquals(listOf("start", "cancel-now"), service.calls)
    }

    @Test
    fun a_restart_after_the_grace_starts_a_new_run() {
        val service = Robolectric.buildService(BackfillRecordingService::class.java).get()

        service.report(AndroidPlaybackState.PLAYING)
        service.report(AndroidPlaybackState.PAUSED)
        looper.idleFor(Duration.ofMillis(ANALYSIS_BACKFILL_STOP_GRACE_MS))
        service.report(AndroidPlaybackState.PLAYING)

        assertEquals(listOf("start", "cancel", "start"), service.calls)
    }

    private fun powerManager(service: BackfillRecordingService) =
        shadowOf(service.getSystemService(Context.POWER_SERVICE) as PowerManager)
}

private class BackfillRecordingService : ReprisePlaybackService() {
    val calls = mutableListOf<String>()

    fun report(state: AndroidPlaybackState) {
        coreListener.onPlaybackChanged(snapshot(state))
    }

    override fun trackAnalysisRequest(trackId: Long, requestGeneration: Long) = Unit

    override fun supersedeForegroundAnalysis(keepTrackId: Long) = Unit

    override fun startAnalysisBackfill() {
        calls += "start"
    }

    override fun cancelAnalysisBackfill() {
        calls += "cancel"
    }

    override fun cancelAnalysisBackfillNow() {
        calls += "cancel-now"
    }
}

private fun snapshot(state: AndroidPlaybackState): AndroidPlaybackSnapshot =
    m9bSnapshot(trackId = 41).copy(state = state)
