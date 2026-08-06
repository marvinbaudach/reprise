package de.reprise.spike

import android.os.Looper
import java.time.Duration
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.android.controller.ServiceController
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidPlaybackSession
import uniffi.reprise_android_ffi.AndroidPlaybackSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackState
import uniffi.reprise_android_ffi.AndroidRepeatMode

/** The sleep timer belongs to the service and these tests create no activity. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class SleepTimerServiceTest {
    private val controllers = mutableListOf<ServiceController<SleepTestPlaybackService>>()

    @After
    fun releaseServices() {
        controllers.forEach(ServiceController<SleepTestPlaybackService>::destroy)
    }

    @Test
    fun fixedTimerFiresWithoutAnActivityFadesPausesAndKeepsThePosition() {
        val service = buildService()
        service.coreListener.onPlaybackChanged(playingSnapshot(positionMs = 42_000))

        service.startSleepTimer(SleepTimerSelection.Minutes(15))
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMinutes(15))
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(4))

        assertEquals("expiry must pause exactly once", 1, service.pausePositions.size)
        assertEquals("pause must keep the last playback position", 42_000L, service.pausePositions.single())
        assertTrue(
            "the eight fade samples must descend rather than cut abruptly",
            service.volumes.take(8).zipWithNext().all { (left, right) -> right < left },
        )
        assertTrue("the fade must reach silence before pausing", service.volumes.contains(0f))
        assertEquals(1f, service.volumes.last(), 0.001f)
        assertFalse(service.sleepTimerState().active)
    }

    @Test
    fun cancellingARunningTimerPreventsItsFadeAndPause() {
        val service = buildService()
        service.coreListener.onPlaybackChanged(playingSnapshot(positionMs = 8_000))
        service.startSleepTimer(SleepTimerSelection.Minutes(15))
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMinutes(5))

        service.cancelSleepTimer()
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMinutes(20))

        assertTrue("cancellation must prevent the later pause", service.pausePositions.isEmpty())
        assertFalse(service.sleepTimerState().active)
    }

    @Test
    fun endOfTrackUsesTheTracksLastSecondsForTheFadeThenPauses() {
        val service = buildService()
        service.coreListener.onPlaybackChanged(
            playingSnapshot(positionMs = 96_000, durationMs = 100_000),
        )

        service.startSleepTimer(SleepTimerSelection.EndOfTrack)
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(4))

        assertEquals(
            "end-of-track must pause at the armed track rather than continue",
            listOf(96_000L),
            service.pausePositions,
        )
        assertTrue(service.volumes.contains(0f))
        assertFalse(service.sleepTimerState().active)
    }

    private fun buildService(): SleepTestPlaybackService {
        val controller = Robolectric.buildService(SleepTestPlaybackService::class.java).create()
        controllers += controller
        return controller.get()
    }
}

private class SleepTestPlaybackService : ReprisePlaybackService() {
    val volumes = mutableListOf<Float>()
    val pausePositions = mutableListOf<Long>()

    override fun openCoreSession(port: Media3PlaybackPort): AndroidPlaybackSession? = null

    override fun applySleepTimerVolume(volume: Float) {
        volumes += volume
    }

    override fun pauseForSleepTimer() {
        pausePositions += sleepTimerPlaybackPositionMs()
    }
}

private fun playingSnapshot(
    positionMs: Long,
    durationMs: Long = 180_000,
) = AndroidPlaybackSnapshot(
    state = AndroidPlaybackState.PLAYING,
    currentIndex = 0UL,
    currentTrackId = 1,
    currentTrackUri = "content://provider/document/song.flac",
    positionMs = positionMs,
    durationMs = durationMs,
    shuffled = false,
    repeat = AndroidRepeatMode.OFF,
    error = null,
)
