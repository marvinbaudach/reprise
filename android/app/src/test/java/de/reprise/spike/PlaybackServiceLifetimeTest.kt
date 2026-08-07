package de.reprise.spike

import android.content.ComponentName
import android.content.Intent
import android.os.Looper
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
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

/**
 * The service's own lifetime, and the screen's part in it.
 *
 * The bug these cover was invisible to every existing test: playback was only
 * ever *bound* to the activity, so a rotation destroyed the service mid-track
 * and the music stopped. The rotation test in `ComposeBehaviorTest` stayed
 * green through all of it because it wires `BrowseScreen` up by hand and never
 * runs `MainActivity`. So these tests run the real activity: `onCreate`,
 * `onStart`, the real `bindService`, the real `onServiceConnected`, and the
 * same `playTracks` the library screen calls — and the real service, built by
 * Robolectric with its real player and session.
 *
 * What they cannot do is turn a phone. That the service, once started and
 * raised into the foreground by Media3, actually survives the rotation is left
 * to the device run.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h1000dp")
class PlaybackServiceLifetimeTest {
    private val application = RuntimeEnvironment.getApplication()
    private val services = mutableListOf<ServiceController<CorelessPlaybackService>>()

    /**
     * Media3 registers a session's id process-wide, and the default id is the
     * empty string. A service left standing would make the next test's session
     * collide with it — so every service built here is torn down, exactly as
     * the system would tear it down.
     */
    @After
    fun releaseTheServices() {
        services.forEach(ServiceController<CorelessPlaybackService>::destroy)
    }

    @Test
    fun openingTheLibraryOnlyBindsTheService() {
        val controller = launchLibraryScreenBoundTo(buildPlaybackService())

        assertEquals(
            "opening the library must not start the service",
            emptyList<Intent>(),
            startIntentsForThePlaybackService(),
        )
        assertEquals(
            "the screen still binds, it just does not start",
            1,
            shadowOf(application).boundServiceConnections.size,
        )

        controller.pause().stop().destroy()
    }

    @Test
    fun aPlaybackCommandStartsTheServiceRatherThanOnlyBindingIt() {
        val controller = launchLibraryScreenBoundTo(buildPlaybackService())
        shadowOf(application).clearStartedServices()

        val failures = mutableListOf<String>()
        controller.get().playTracks(PlaybackSelection(listOf(aTrack()), 0)) { failures += it }

        // The command reached the bound service rather than falling at the
        // "still connecting" hurdle: without this the test could pass while
        // `onServiceConnected` never ran, which is how the old rotation test
        // managed to be green about a broken screen. The core is absent here
        // by construction — its native library cannot load on the JVM.
        assertEquals(
            listOf("Could not play Probe Song: Core playback session is not ready"),
            failures,
        )
        assertEquals(
            "a playback command has to start the service, not just bind it",
            1,
            startIntentsForThePlaybackService().size,
        )

        controller.pause().stop().destroy()
    }

    @Test
    fun leavingTheScreenUnbindsTheServiceWithoutStoppingIt() {
        val controller = launchLibraryScreenBoundTo(buildPlaybackService())
        controller.get().playTracks(PlaybackSelection(listOf(aTrack()), 0)) {}

        controller.pause().stop()

        assertTrue(
            "the screen has to let go of the service",
            shadowOf(application).boundServiceConnections.isEmpty(),
        )
        assertNull(
            "unbinding must not take the started service with it",
            shadowOf(application).nextStoppedService,
        )

        controller.destroy()
    }

    @Test
    fun theServiceHandsItsSessionToMedia3() {
        val service = buildPlaybackService()

        // Media3 only notifies, and only raises the service into the
        // foreground, for sessions the service has registered with it. Without
        // this the platform saw a playing session while the service stayed an
        // ordinary bound one, and there was no notification at all.
        assertEquals(
            "Media3 has to be told about the session, or it never notifies for it",
            1,
            service.sessions.size,
        )
        assertTrue(
            "the registered session has to be the one that carries the transport",
            service.sessions.first().player is CoreControlledPlayer,
        )
    }

    @Test
    fun theServiceStopsItselfOnceTheQueueHasRunOut() {
        val service = buildPlaybackService()

        service.coreListener.onPlaybackChanged(snapshot(AndroidPlaybackState.PLAYING, trackId = 7))
        assertFalse(
            "a playing service must stay",
            shadowOf(service).isStoppedBySelf,
        )

        service.coreListener.onPlaybackChanged(snapshot(AndroidPlaybackState.STOPPED, trackId = null))
        assertTrue(
            "a service with an empty queue has nothing left to keep alive",
            shadowOf(service).isStoppedBySelf,
        )
    }

    @Test
    fun theServiceStaysWhilePlaybackIsMerelyPaused() {
        val service = buildPlaybackService()

        service.coreListener.onPlaybackChanged(snapshot(AndroidPlaybackState.PAUSED, trackId = 7))

        assertFalse(
            "a paused track is still a queue, and its notification still has work to do",
            shadowOf(service).isStoppedBySelf,
        )
    }

    private fun buildPlaybackService(): ReprisePlaybackService =
        buildPlaybackServiceController().get()

    private fun buildPlaybackServiceController(): ServiceController<CorelessPlaybackService> =
        Robolectric.buildService(CorelessPlaybackService::class.java)
            .create()
            .also(services::add)

    /**
     * Brings up the real activity bound to [service], the way the system would.
     *
     * Stops short of `resume`: composing the library screen needs the native
     * library, which the JVM cannot load. Everything this covers — binding,
     * connecting, the playback commands — happens before that.
     */
    private fun launchLibraryScreenBoundTo(
        service: ReprisePlaybackService,
    ): ActivityController<MainActivity> {
        val binder = service.onBind(Intent(ReprisePlaybackService.LOCAL_BIND_ACTION))
        shadowOf(application).setComponentNameAndServiceForBindService(
            ComponentName(application, ReprisePlaybackService::class.java),
            binder,
        )
        val controller = Robolectric.buildActivity(MainActivity::class.java).create().start()
        // Robolectric delivers the connection through the main looper, and so
        // does the framework. Without this `onServiceConnected` never runs.
        shadowOf(Looper.getMainLooper()).idle()
        return controller
    }

    /**
     * The service intents the screen sent, minus the bind.
     *
     * Robolectric records a bound service's intent in the same queue as a
     * started one, and both name the same component; only the bind carries an
     * action.
     */
    private fun startIntentsForThePlaybackService(): List<Intent> =
        generateSequence { shadowOf(application).nextStartedService }
            .filter { intent ->
                intent.component?.className == ReprisePlaybackService::class.java.name &&
                    intent.action == null
            }
            .toList()
}

/**
 * The real service with the one step the JVM cannot take left out: opening the
 * core's session needs the `.so`, which only exists for the device.
 */
private class CorelessPlaybackService : ReprisePlaybackService() {
    override fun openCoreSession(port: Media3PlaybackPort): AndroidPlaybackSession? = null
}

private fun aTrack() = LibraryTrack(
    id = 1,
    uri = "content://provider/document/song.flac",
    title = "Probe Song",
    artist = "Artist",
    album = "Album",
    durationMs = 1_000,
    playCount = 0,
    rating = 0,
)

private fun snapshot(state: AndroidPlaybackState, trackId: Long?) = AndroidPlaybackSnapshot(
    state = state,
    currentIndex = trackId?.let { 0UL },
    currentTrackId = trackId,
    currentTrackUri = trackId?.let { "content://provider/document/song.flac" },
    positionMs = 0,
    durationMs = 0,
    automaticAdvanceCount = 0u,
    shuffled = false,
    repeat = AndroidRepeatMode.OFF,
    error = null,
)
