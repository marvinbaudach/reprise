package de.reprise.spike

import android.app.Application
import android.content.ComponentName
import android.content.Intent
import android.os.Looper
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.android.controller.ActivityController
import org.robolectric.android.controller.ServiceController
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidPlaybackSession
import uniffi.reprise_android_ffi.AndroidPlaybackSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackState
import uniffi.reprise_android_ffi.AndroidRepeatMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class MainActivityPlaybackChannelTest {
    private val activities = mutableListOf<ActivityController<MainActivity>>()
    private val services = mutableListOf<ServiceController<PlaybackChannelService>>()

    @After
    fun releaseTheActivityAndService() {
        activities.forEach { controller ->
            runCatching { controller.pause().stop().destroy() }
        }
        services.forEach(ServiceController<PlaybackChannelService>::destroy)
    }

    @Test
    fun anAutomaticTrackTransitionReachesTheScreen() {
        val service = buildService()
        val activity = launchBoundActivity(service)
        service.publish(activitySnapshot(trackId = 1, positionMs = 10_000))
        idleMainLooper()

        service.publish(activitySnapshot(trackId = 2, positionMs = 22_000))
        idleMainLooper()

        val state = activity.currentPlaybackState
        assertEquals(2L, state.currentTrackId)
        assertEquals(22_000L, state.positionMs)
        assertTrue(state.visualizerActive)
    }

    @Test
    fun stopAndStartRepublishesTheStateThatArrivedWhileTheScreenWasAway() {
        val service = buildService()
        val controller = launchBoundActivityController(service)
        service.publish(activitySnapshot(trackId = 1, positionMs = 10_000))
        idleMainLooper()

        controller.stop()
        service.publish(activitySnapshot(trackId = 2, positionMs = 28_000))
        controller.start()
        idleMainLooper()

        val state = controller.get().currentPlaybackState
        assertEquals(2L, state.currentTrackId)
        assertEquals(28_000L, state.positionMs)
        assertTrue(state.visualizerActive)
    }

    @Test
    fun aCommandIssuedRightAfterRestartReachesTheService() {
        val service = buildService()
        val controller = launchBoundActivityController(service)
        controller.stop().start()
        idleMainLooper()
        val errors = mutableListOf<String>()

        controller.get().playTracks(
            PlaybackSelection(listOf(channelTrack()), 0),
            errors::add,
        )

        assertFalse(errors.any { error -> error.contains("playback is still connecting") })
    }

    @Test
    fun losingTheServiceDoesNotClaimNothingIsPlaying() {
        val service = buildService()
        val activity = launchBoundActivity(service)
        service.publish(activitySnapshot(trackId = 3, positionMs = 31_000))
        idleMainLooper()

        val connection = shadowOf(application()).boundServiceConnections.single()
        connection.onServiceDisconnected(playbackComponent())
        idleMainLooper()

        val state = activity.currentPlaybackState
        assertEquals(3L, state.currentTrackId)
        assertEquals(31_000L, state.positionMs)
        assertTrue(state.visualizerActive)
    }

    private fun buildService(): PlaybackChannelService =
        Robolectric.buildService(PlaybackChannelService::class.java)
            .create()
            .also(services::add)
            .get()

    private fun launchBoundActivity(service: ReprisePlaybackService): MainActivity =
        launchBoundActivityController(service).get()

    private fun launchBoundActivityController(
        service: ReprisePlaybackService,
    ): ActivityController<MainActivity> {
        shadowOf(application()).setComponentNameAndServiceForBindService(
            playbackComponent(),
            service.onBind(Intent(ReprisePlaybackService.LOCAL_BIND_ACTION)),
        )
        return Robolectric.buildActivity(MainActivity::class.java)
            .create()
            .start()
            .also(activities::add)
            .also { idleMainLooper() }
    }

    private fun application(): Application = RuntimeEnvironment.getApplication()

    private fun playbackComponent() =
        ComponentName(application(), ReprisePlaybackService::class.java)

    private fun idleMainLooper() {
        shadowOf(Looper.getMainLooper()).idle()
    }
}

private class PlaybackChannelService : ReprisePlaybackService() {
    override fun openCoreSession(port: Media3PlaybackPort): AndroidPlaybackSession? = null

    fun publish(snapshot: AndroidPlaybackSnapshot) {
        coreListener.onPlaybackChanged(snapshot)
    }
}

private fun activitySnapshot(trackId: Long, positionMs: Long) = AndroidPlaybackSnapshot(
    state = AndroidPlaybackState.PLAYING,
    currentIndex = 0UL,
    currentTrackId = trackId,
    currentTrackUri = "content://provider/document/$trackId.flac",
    positionMs = positionMs,
    durationMs = 180_000,
    automaticAdvanceCount = 0u,
    shuffled = false,
    repeat = AndroidRepeatMode.OFF,
    error = null,
)

private fun channelTrack() = LibraryTrack(
    id = 1,
    uri = "content://provider/document/1.flac",
    title = "Channel Song",
    artist = "Artist",
    album = "Album",
    durationMs = 180_000,
    playCount = 0,
    rating = 0,
)
