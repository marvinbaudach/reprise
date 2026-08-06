package de.reprise.spike

import android.os.Looper
import java.time.Duration
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import org.junit.After
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

/** Timer presentation uses MainActivity.onCreate, the real bind, and recreation. */
@RunWith(RobolectricTestRunner::class)
@Config(
    sdk = [36],
    qualifiers = "w412dp-h916dp-port",
    application = ConfigurationTestApplication::class,
)
class MainActivitySleepTimerTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val application: ConfigurationTestApplication
        get() = RuntimeEnvironment.getApplication() as ConfigurationTestApplication

    @After
    fun releaseTheService() {
        application.releaseService()
    }

    @Test
    fun sheetOffersEveryDurationShowsRemainingAcrossRecreateAndCancels() {
        application.service.publish(m9bSnapshot(trackId = 1))
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
        compose.onNodeWithTag("library-mini-player").performClick()

        compose.onNodeWithContentDescription("Set sleep timer").performClick()
        listOf("15 minutes", "30 minutes", "45 minutes", "60 minutes", "End of track")
            .forEach { choice -> compose.onNodeWithText(choice).assertIsDisplayed() }
        compose.onNodeWithText("15 minutes").performClick()
        compose.onNodeWithContentDescription("Sleep timer, 15:00 remaining").assertIsDisplayed()

        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(61))
        compose.waitForIdle()
        val remainingBeforeRecreate = checkNotNull(
            application.service.sleepTimerState().remainingSeconds,
        )
        assertTrue(remainingBeforeRecreate in 839L..840L)
        compose.onNodeWithContentDescription(
            timerDescription(remainingBeforeRecreate),
        ).assertIsDisplayed()

        compose.activityRule.scenario.recreate()
        shadowOf(Looper.getMainLooper()).idle()
        application.service.republish()
        shadowOf(Looper.getMainLooper()).idle()
        compose.waitForIdle()
        val remainingAfterRecreate = checkNotNull(
            application.service.sleepTimerState().remainingSeconds,
        )
        compose.onNodeWithContentDescription(
            timerDescription(remainingAfterRecreate),
        ).assertIsDisplayed()

        compose.onNodeWithContentDescription(timerDescription(remainingAfterRecreate)).performClick()
        compose.onNodeWithText("Cancel timer").performClick()
        compose.onNodeWithContentDescription("Set sleep timer").assertIsDisplayed()
    }
}

private fun timerDescription(seconds: Long): String =
    "Sleep timer, ${seconds / 60}:${(seconds % 60).toString().padStart(2, '0')} remaining"
