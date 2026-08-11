package de.reprise.spike

import android.os.Looper
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextContains
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidPlaybackState

@RunWith(RobolectricTestRunner::class)
@Config(
    sdk = [36],
    qualifiers = "w412dp-h916dp-port",
    application = DelayedTrackConfigurationTestApplication::class,
)
class MainActivityPlayViewStabilityTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val application: DelayedTrackConfigurationTestApplication
        get() = RuntimeEnvironment.getApplication() as DelayedTrackConfigurationTestApplication

    @After
    fun releaseTheService() {
        application.releaseService()
    }

    @Test
    fun delayedTrackAnswerKeepsThePlayViewAndMiniPlayerOnTheirLastAnsweredRow() {
        publishTrack(1)
        compose.onNodeWithTag("library-mini-player").performClick()
        compose.onNodeWithTag("now-playing-title").assertTextContains("Rotation Song 1")

        application.deferTrack(2)
        publishTrack(2)

        compose.onNodeWithTag("now-playing-gestures").assertIsDisplayed()
        compose.onNodeWithTag("now-playing-title").assertTextContains("Rotation Song 1")

        compose.activity.onBackPressedDispatcher.onBackPressed()
        compose.waitForIdle()
        compose.onNodeWithTag("library-mini-player")
            .assertIsDisplayed()
            .assertTextContains("Rotation Song 1")
    }

    @Test
    fun stalePlayViewActionsStayDisabledUntilTheAnswerAndStopBlanksBothRows() {
        publishTrack(1)
        compose.onNodeWithTag("library-mini-player").performClick()
        application.deferTrack(2)
        publishTrack(2)

        compose.onNodeWithTag("now-playing-heart")
            .assertIsDisplayed()
            .assertIsNotEnabled()
            .performClick()
        compose.onNodeWithTag("now-playing-overflow").assertIsNotEnabled()
        assertEquals(emptyList<Pair<Long, Int>>(), application.controls.ratingRequests)

        application.completeTrack(2)
        compose.waitForIdle()

        compose.onNodeWithTag("now-playing-title").assertTextContains("Rotation Song 2")
        compose.onNodeWithTag("now-playing-heart").assertIsEnabled()
        compose.onNodeWithTag("now-playing-overflow").assertIsEnabled()

        application.deferTrack(3)
        publishTrack(3)
        application.service.publish(
            m9bSnapshot(3).copy(
                state = AndroidPlaybackState.STOPPED,
                currentIndex = null,
                currentTrackId = null,
                currentTrackUri = null,
                positionMs = 0,
                durationMs = 0,
            ),
        )
        idlePlayback()

        compose.onNodeWithTag("now-playing-gestures").assertDoesNotExist()
        compose.onNodeWithTag("library-mini-player").assertDoesNotExist()
    }

    private fun publishTrack(trackId: Long) {
        application.service.publish(m9bSnapshot(trackId))
        idlePlayback()
    }

    private fun idlePlayback() {
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
    }
}

internal class DelayedTrackConfigurationTestApplication : ConfigurationTestApplication() {
    private val deferredTrackIds = mutableSetOf<Long>()
    private val pendingAnswers = mutableMapOf<Long, MutableList<() -> Unit>>()

    fun deferTrack(trackId: Long) {
        deferredTrackIds += trackId
    }

    fun completeTrack(trackId: Long) {
        deferredTrackIds -= trackId
        pendingAnswers.remove(trackId).orEmpty().forEach { answer -> answer() }
    }

    override fun mainActivitySurface(): MainActivitySurfaceDependencies {
        val base = super.mainActivitySurface()
        return base.copy(
            loadTrack = { trackId, deliver ->
                base.loadTrack(trackId) { track ->
                    if (trackId in deferredTrackIds) {
                        pendingAnswers.getOrPut(trackId, ::mutableListOf) += { deliver(track) }
                    } else {
                        deliver(track)
                    }
                }
            },
        )
    }
}
