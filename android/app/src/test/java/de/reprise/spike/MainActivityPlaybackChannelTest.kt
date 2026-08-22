package de.reprise.spike

import android.app.Activity
import android.app.Application
import android.content.ComponentName
import android.content.Intent
import android.os.Bundle
import android.os.Looper
import android.util.Log
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import java.util.PriorityQueue
import java.util.concurrent.TimeUnit
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TestRule
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.android.controller.ActivityController
import org.robolectric.android.controller.ServiceController
import org.robolectric.annotation.Config
import org.robolectric.annotation.LooperMode
import org.robolectric.shadows.ShadowLog
import uniffi.reprise_android_ffi.AndroidPlaybackSession
import uniffi.reprise_android_ffi.AndroidPlaybackSnapshot
import uniffi.reprise_android_ffi.AndroidPlaybackState
import uniffi.reprise_android_ffi.AndroidRepeatMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class MainActivityPlaybackChannelTest {
    private val recreationCompose = createAndroidComposeRule<MainActivity>()

    @get:Rule
    val recreationRule = TestRule { base, description ->
        if (description.methodName == "theUiStateSurvivesAnActivityRecreationWhilePlaying") {
            recreationCompose.apply(base, description)
        } else {
            base
        }
    }

    private val activities = mutableListOf<ActivityController<MainActivity>>()
    private val services = mutableListOf<ServiceController<PlaybackChannelService>>()

    @After
    fun releaseTheActivityAndService() {
        activities.forEach { controller ->
            runCatching { controller.pause().stop().destroy() }
        }
        services.forEach(ServiceController<PlaybackChannelService>::destroy)
        (application() as? ConfigurationTestApplication)?.releaseService()
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

    @Test
    @LooperMode(LooperMode.Mode.LEGACY)
    fun aBindThatNeverConnectsIsRetriedAndLogged() {
        val shadowApplication = shadowOf(application())
        shadowOf(Looper.getMainLooper()).pause()
        Robolectric.buildActivity(MainActivity::class.java)
            .create()
            .start()
            .also(activities::add)
        discardImmediateMainLooperWork()
        shadowApplication.declareActionUnbindable(ReprisePlaybackService.LOCAL_BIND_ACTION)

        shadowOf(Looper.getMainLooper()).idleFor(
            PLAYBACK_BIND_WATCHDOG_MS + 1,
            TimeUnit.MILLISECONDS,
        )

        assertEquals(2, shadowApplication.boundServiceConnections.size)
        assertPlaybackBindFailureWasLogged()
    }

    @Test
    fun aRefusedBindIsRecorded() {
        shadowOf(application()).declareActionUnbindable(ReprisePlaybackService.LOCAL_BIND_ACTION)

        Robolectric.buildActivity(MainActivity::class.java)
            .create()
            .start()
            .also(activities::add)

        assertPlaybackBindFailureWasLogged()
    }

    @Test
    @Config(sdk = [36], application = ConfigurationTestApplication::class)
    fun theUiStateSurvivesAnActivityRecreationWhilePlaying() {
        val service = (application() as ConfigurationTestApplication).service
        service.publish(activitySnapshot(trackId = 6, positionMs = 12_000))
        idleMainLooper()
        recreationCompose.waitForIdle()
        assertEquals(6L, recreationCompose.activity.currentPlaybackState.currentTrackId)

        val publishOnStop = PublishPlaybackOnStop(
            service,
            activitySnapshot(trackId = 7, positionMs = 24_000),
        )
        application().registerActivityLifecycleCallbacks(publishOnStop)
        RuntimeEnvironment.setQualifiers("w916dp-h412dp-land")
        try {
            recreationCompose.activityRule.scenario.recreate()
        } finally {
            application().unregisterActivityLifecycleCallbacks(publishOnStop)
        }
        idleMainLooper()
        recreationCompose.waitForIdle()

        assertTrue(publishOnStop.published)
        val state = recreationCompose.activity.currentPlaybackState
        assertEquals(7L, state.currentTrackId)
        assertEquals(24_000L, state.positionMs)
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

    private fun assertPlaybackBindFailureWasLogged() {
        assertTrue(
            ShadowLog.getLogsForTag("RepriseScan").any { item ->
                item.type == Log.WARN && item.msg.contains(PLAYBACK_BIND_FAILURE_LOG)
            },
        )
    }

    /** Leaves delayed work in place while withholding Robolectric's bind callback. */
    @Suppress("UNCHECKED_CAST")
    private fun discardImmediateMainLooperWork() {
        val scheduler = shadowOf(Looper.getMainLooper()).scheduler
        val runnablesField = scheduler.javaClass.getDeclaredField("runnables").apply {
            isAccessible = true
        }
        val runnables = runnablesField.get(scheduler) as PriorityQueue<Any>
        val immediate = runnables.filter { scheduled ->
            val scheduledTime = scheduled.javaClass.getDeclaredField("scheduledTime").apply {
                isAccessible = true
            }.getLong(scheduled)
            scheduledTime <= scheduler.currentTime
        }
        immediate.forEach(runnables::remove)
    }
}

private class PlaybackChannelService : ReprisePlaybackService() {
    override fun openCoreSession(port: Media3PlaybackPort): AndroidPlaybackSession? = null

    fun publish(snapshot: AndroidPlaybackSnapshot) {
        coreListener.onPlaybackChanged(snapshot)
    }
}

private class PublishPlaybackOnStop(
    private val service: ConfigurationTestPlaybackService,
    private val snapshot: AndroidPlaybackSnapshot,
) : Application.ActivityLifecycleCallbacks {
    var published = false
        private set

    override fun onActivityStopped(activity: Activity) {
        if (activity !is MainActivity || published) return
        service.publish(snapshot)
        published = true
    }

    override fun onActivityCreated(activity: Activity, savedInstanceState: Bundle?) = Unit
    override fun onActivityStarted(activity: Activity) = Unit
    override fun onActivityResumed(activity: Activity) = Unit
    override fun onActivityPaused(activity: Activity) = Unit
    override fun onActivitySaveInstanceState(activity: Activity, outState: Bundle) = Unit
    override fun onActivityDestroyed(activity: Activity) = Unit
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
