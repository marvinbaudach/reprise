package io.github.marvinbaudach.reprise

import android.os.Looper
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.media3.common.PlaybackException
import androidx.lifecycle.Lifecycle
import java.io.FileNotFoundException
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(
    sdk = [36],
    qualifiers = "w412dp-h916dp-port",
    application = ConfigurationTestApplication::class,
)
class PlaybackFaultNoticeTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val application: ConfigurationTestApplication
        get() = RuntimeEnvironment.getApplication() as ConfigurationTestApplication

    @After
    fun releaseTheService() {
        application.releaseService()
    }

    @Test
    fun firstSnapshotArmsTheFaultNoticeWithoutRaisingIt() {
        val update = updatePlaybackFaultNotice(
            previousCount = null,
            currentCount = 4uL,
            text = "Track unavailable — skipped",
            previousMessage = null,
        )

        assertEquals(4uL, update.observedCount)
        assertNull(update.message)
    }

    @Test
    fun aLaterFaultNoticeCountRaisesTheSnapshotText() {
        val update = updatePlaybackFaultNotice(
            previousCount = 4uL,
            currentCount = 5uL,
            text = "Track unavailable — skipped",
            previousMessage = null,
        )

        assertEquals(5uL, update.observedCount)
        assertEquals(TransientMessage("Track unavailable — skipped"), update.message)
    }

    @Test
    fun repeatedFaultTextGetsItsOwnDismissalLifetime() {
        val first = updatePlaybackFaultNotice(
            previousCount = 4uL,
            currentCount = 5uL,
            text = "Track unavailable — skipped",
            previousMessage = null,
        ).message
        val second = updatePlaybackFaultNotice(
            previousCount = 5uL,
            currentCount = 6uL,
            text = "Track unavailable — skipped",
            previousMessage = first,
        ).message

        assertEquals(first?.text, second?.text)
        assertNotEquals(first, second)
    }

    @Test
    fun steadySnapshotsKeepTheRaisedNoticeAlive() {
        val message = TransientMessage("Track unavailable — skipped")

        val update = updatePlaybackFaultNotice(
            previousCount = 5uL,
            currentCount = 5uL,
            text = "Track unavailable — skipped",
            previousMessage = message,
        )

        assertEquals(5uL, update.observedCount)
        assertEquals(message, update.message)
    }

    @Test
    fun aSteadySnapshotWithoutNoticeTextClearsTheMessage() {
        val update = updatePlaybackFaultNotice(
            previousCount = 5uL,
            currentCount = 5uL,
            text = null,
            previousMessage = TransientMessage("Track unavailable — skipped"),
        )

        assertEquals(5uL, update.observedCount)
        assertNull(update.message)
    }

    @Test
    fun aMissingFileIsClassifiedFromTheMedia3CodeOrItsCauseChain() {
        val typed = PlaybackException(
            "Missing",
            null,
            PlaybackException.ERROR_CODE_IO_FILE_NOT_FOUND,
        )
        val caused = PlaybackException(
            "Source error",
            IllegalStateException("wrapper", FileNotFoundException("gone")),
            PlaybackException.ERROR_CODE_IO_UNSPECIFIED,
        )
        val other = PlaybackException(
            "Decoder error",
            IllegalStateException("broken"),
            PlaybackException.ERROR_CODE_DECODING_FAILED,
        )

        assertTrue(isMissingFilePlaybackError(typed))
        assertTrue(isMissingFilePlaybackError(caused))
        assertFalse(isMissingFilePlaybackError(other))
    }

    @Test
    fun dockModeShowsEachFaultNoticeExactlyOnce() {
        application.service.publish(m9bSnapshot(1))
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
        RuntimeEnvironment.setQualifiers("w916dp-h412dp-land")
        compose.activityRule.scenario.recreate()
        shadowOf(Looper.getMainLooper()).idle()
        application.service.republish()
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
        compose.onNodeWithText("Dock mode").performClick()

        application.service.publish(
            m9bSnapshot(1).copy(
                faultNotice = "Track unavailable — skipped",
                faultNoticeCount = 1u,
            ),
        )
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()

        compose.onNodeWithText("Track unavailable — skipped").assertIsDisplayed()
        compose.onAllNodesWithText("Track unavailable — skipped").assertCountEquals(1)
    }

    @Test
    fun stoppingTheScreenClearsAnActiveFaultNotice() {
        application.service.publish(m9bSnapshot(1))
        shadowOf(Looper.getMainLooper()).idle()
        application.service.publish(
            m9bSnapshot(1).copy(
                faultNotice = "Track unavailable — skipped",
                faultNoticeCount = 1u,
            ),
        )
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
        compose.onNodeWithText("Track unavailable — skipped").assertIsDisplayed()
        assertEquals(
            "Track unavailable — skipped",
            compose.activity.currentPlaybackState.faultNotice?.text,
        )

        compose.activityRule.scenario.moveToState(Lifecycle.State.CREATED)
        shadowOf(Looper.getMainLooper()).idle()

        assertNull(compose.activity.currentPlaybackState.faultNotice)
        compose.onNodeWithText("Track unavailable — skipped").assertDoesNotExist()
    }
}
